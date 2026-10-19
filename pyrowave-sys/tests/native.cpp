#include <cstdio>
#include <climits>
#include <cstring>
#include <stdexcept>
#include <vector>

// Inspect native caches without adding test hooks or handles to the public API.
#include "pyrowave_c.cpp"
#include "pyrowave_common.hpp"

static void require(bool condition, const char *message)
{
	if (!condition)
		throw std::runtime_error(message);
}

struct Fixture
{
	pyrowave_device device = nullptr;
	pyrowave_encoder encoder = nullptr;

	Fixture(pyrowave_chroma_subsampling chroma = PYROWAVE_CHROMA_SUBSAMPLING_444)
	{
		require(pyrowave_create_default_device(&device) == PYROWAVE_SUCCESS, "creating Vulkan device");
		pyrowave_encoder_create_info info = {device, 128, 128, chroma};
		if (pyrowave_encoder_create(&info, &encoder) != PYROWAVE_SUCCESS)
		{
			pyrowave_device_destroy(device);
			throw std::runtime_error("creating encoder");
		}
	}

	~Fixture()
	{
		if (encoder)
			pyrowave_encoder_destroy(encoder);
		if (device)
			pyrowave_device_destroy(device);
	}
};

static void test_frame_context_bounds()
{
	pyrowave_device_opaque device;
	pyrowave_encoder_opaque encoder;
	pyrowave_decoder_opaque decoder;
	decoder.pyro_device = &device;
	pyrowave_cpu_buffer buffers = {};
	for (int context : {INT_MIN, -1, PYROWAVE_MAX_FRAME_CONTEXTS, INT_MAX})
	{
		require(pyrowave_encoder_set_frame_context(&encoder, context) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting invalid encoder frame context");
		require(pyrowave_decoder_decode_cpu_buffer_async(&decoder, &buffers, context) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting invalid asynchronous decoder frame context");
		require(pyrowave_decoder_decode_cpu_buffer_complete(&decoder, &buffers, context) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting invalid decoder completion frame context");
	}
	for (int context : {0, 1})
	{
		require(pyrowave_encoder_set_frame_context(&encoder, context) == PYROWAVE_SUCCESS,
		        "accepting valid encoder frame context");
		require(encoder.context == context, "selecting requested frame context");
	}
}

static void test_cpu_format_changes()
{
	for (auto chroma : {PYROWAVE_CHROMA_SUBSAMPLING_420, PYROWAVE_CHROMA_SUBSAMPLING_444})
	{
		Fixture fixture(chroma);
		pyrowave_decoder_create_info info = {fixture.device, 128, 128, chroma, false};
		pyrowave_decoder raw_decoder = nullptr;
		require(pyrowave_decoder_create(&info, &raw_decoder) == PYROWAVE_SUCCESS, "creating format-change decoder");
		struct DeleteDecoder { void operator()(pyrowave_decoder decoder) { pyrowave_decoder_destroy(decoder); } };
		std::unique_ptr<pyrowave_decoder_opaque, DeleteDecoder> decoder(raw_decoder);
		const bool subsampled = chroma == PYROWAVE_CHROMA_SUBSAMPLING_420;
		for (int context : {0, 1})
		{
			require(pyrowave_encoder_set_frame_context(fixture.encoder, context) == PYROWAVE_SUCCESS,
			        "selecting format-change frame context");
			for (int depth : {8, 16, 10, 8, 0, 16, 8})
			{
				if (depth == 0 && !subsampled)
					continue;
				const bool nv12 = depth == 0;
				const size_t bpp = depth > 8 ? 2 : 1;
				const unsigned maximum = depth == 10 ? 1023 : depth == 16 ? 65535 : 255;
				pyrowave_cpu_buffer input = {};
				input.width = input.height = 128;
				input.format = nv12 ? PYROWAVE_CPU_BUFFER_FORMAT_NV12 : depth == 10
				    ? (subsampled ? PYROWAVE_CPU_BUFFER_FORMAT_YUV420P10 : PYROWAVE_CPU_BUFFER_FORMAT_YUV444P10)
				    : depth == 16
				    ? (subsampled ? PYROWAVE_CPU_BUFFER_FORMAT_YUV420P16 : PYROWAVE_CPU_BUFFER_FORMAT_YUV444P16)
				    : (subsampled ? PYROWAVE_CPU_BUFFER_FORMAT_YUV420P : PYROWAVE_CPU_BUFFER_FORMAT_YUV444P);
				std::vector<uint16_t> samples[3];
				for (int plane = 0; plane < (nv12 ? 2 : 3); plane++)
				{
					const size_t width = plane && subsampled ? 64 : 128;
					const size_t height = plane && subsampled ? 64 : 128;
					const size_t components = nv12 && plane == 1 ? 2 : 1;
					input.row_stride_in_bytes[plane] = width * bpp * components;
					input.plane_size_in_bytes[plane] = input.row_stride_in_bytes[plane] * height;
					samples[plane].resize((input.plane_size_in_bytes[plane] + 1) / 2);
					input.data[plane] = samples[plane].data();
					for (size_t i = 0; i < width * height * components; i++)
					{
						const unsigned value = plane == 0 ? 96 : (nv12 && plane == 1 ? (i % 2 ? 176 : 80) : plane == 1 ? 112 : 144);
						if (bpp == 2)
							samples[plane][i] = uint16_t((value * maximum + 127) / 255);
						else
							static_cast<uint8_t *>(input.data[plane])[i] = uint8_t(value);
					}
				}
				pyrowave_rate_control rate = {65536};
				require(pyrowave_encoder_encode_cpu(fixture.encoder, &input, &rate) == PYROWAVE_SUCCESS,
				        "encoding after CPU plane format change");
				size_t count = 0;
				require(pyrowave_encoder_compute_num_packets(fixture.encoder, 65536, &count) == PYROWAVE_SUCCESS,
				        "waiting for format-change encode");
				std::vector<pyrowave_packet> packets(count);
				std::vector<uint8_t> stream(65536);
				require(pyrowave_encoder_packetize(fixture.encoder, packets.data(), 65536, &count,
				                                  stream.data(), stream.size()) == PYROWAVE_SUCCESS,
				        "packetizing format-change encode");
				for (size_t i = 0; i < count; i++)
					require(pyrowave_decoder_push_packet(decoder.get(), stream.data() + packets[i].offset, packets[i].size)
					        == PYROWAVE_SUCCESS, "queuing format-change packets");
				pyrowave_cpu_buffer output = {};
				output.width = output.height = 128;
				output.format = bpp == 2
				    ? (subsampled ? PYROWAVE_CPU_BUFFER_FORMAT_YUV420P16 : PYROWAVE_CPU_BUFFER_FORMAT_YUV444P16)
				    : (subsampled ? PYROWAVE_CPU_BUFFER_FORMAT_YUV420P : PYROWAVE_CPU_BUFFER_FORMAT_YUV444P);
				std::vector<uint16_t> decoded[3];
				for (int plane = 0; plane < 3; plane++)
				{
					const size_t width = plane && subsampled ? 64 : 128;
					const size_t height = plane && subsampled ? 64 : 128;
					output.row_stride_in_bytes[plane] = width * bpp;
					output.plane_size_in_bytes[plane] = width * height * bpp;
					decoded[plane].resize((output.plane_size_in_bytes[plane] + 1) / 2 + 32, 0xa5a5);
					output.data[plane] = decoded[plane].data();
				}
				require(pyrowave_decoder_decode_cpu_buffer_async(decoder.get(), &output, context) == PYROWAVE_SUCCESS,
				        "decoding after CPU plane format change");
				require(pyrowave_decoder_decode_cpu_buffer_complete(decoder.get(), &output, context) == PYROWAVE_SUCCESS,
				        "completing format-change readback");
				for (int plane = 0; plane < 3; plane++)
				{
					const unsigned value = plane == 0 ? 96 : nv12 ? (plane == 1 ? 80 : 176) : plane == 1 ? 112 : 144;
					const unsigned expected = bpp == 2 ? value * 257 : value;
					const unsigned tolerance = bpp == 2 ? 768 : 3;
					for (size_t i = 0; i < output.plane_size_in_bytes[plane] / bpp; i++)
					{
						const unsigned actual = bpp == 2 ? decoded[plane][i] : static_cast<uint8_t *>(output.data[plane])[i];
						require(actual + tolerance >= expected && expected + tolerance >= actual,
						        "CPU format change corrupted decoded samples");
					}
					const auto *bytes = static_cast<const uint8_t *>(output.data[plane]);
					for (size_t i = output.plane_size_in_bytes[plane]; i < decoded[plane].size() * 2; i++)
						require(bytes[i] == 0xa5, "CPU format change overwrote readback tail");
				}
			}
		}
	}
}

static void test_scaled_encoding(bool error_cleanup)
{
	Fixture fixture;
	auto &device = fixture.device->device;
	std::vector<uint32_t> pixels(128 * 128, 0xff808080);
	ImageInitialData initial = {pixels.data(), 128, 128};
	auto image_info = ImageCreateInfo::immutable_2d_image(128, 128, VK_FORMAT_R8G8B8A8_UNORM);
	image_info.initial_layout = VK_IMAGE_LAYOUT_GENERAL;
	image_info.misc |= IMAGE_MISC_EXTERNAL_MEMORY_BIT;
	image_info.external.memory_handle_type = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD_BIT;
	auto image = device.create_image(image_info, &initial);
	require(bool(image), "creating external RGB allocation");
	auto prepare = device.request_command_buffer();
	prepare->release_image_barrier(*image, VK_IMAGE_LAYOUT_GENERAL, VK_IMAGE_LAYOUT_GENERAL,
	                              VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, VK_ACCESS_2_MEMORY_WRITE_BIT,
	                              VK_QUEUE_FAMILY_EXTERNAL);
	Fence ready;
	device.submit(prepare, &ready);
	ready->wait();
	pyrowave_image_opaque source;
	source.device = &device;
	source.img = image;
	pyrowave_scaled_encode_info scaling = {};
	require(pyrowave_image_get_image_view(&source, VK_IMAGE_ASPECT_COLOR_BIT,
	                                     VK_IMAGE_USAGE_SAMPLED_BIT, &scaling.view) == PYROWAVE_SUCCESS,
	        "creating RGB sampling view");
	scaling.input_color_space = VK_COLOR_SPACE_SRGB_NONLINEAR_KHR;
	scaling.output_color_space = VK_COLOR_SPACE_SRGB_NONLINEAR_KHR;
	scaling.intermediate_plane_format = VK_FORMAT_R8_UNORM;
	scaling.ycbcr_chroma_midpoint = 128.0f / 255.0f;
	pyrowave_gpu_external_reference reference = {&source, VK_QUEUE_FAMILY_EXTERNAL};
	pyrowave_gpu_sync_operation ownership = {};
	ownership.images = &reference;
	ownership.num_images = 1;
	auto acquire = ownership;
	pyrowave_rate_control rate = {65536};

	if (error_cleanup)
	{
		auto semaphore = device.request_semaphore(VK_SEMAPHORE_TYPE_TIMELINE);
		ownership.sync.semaphore = semaphore->get_semaphore();
		ownership.sync.value = 1;
		scaling.intermediate_plane_format = VK_FORMAT_UNDEFINED;
		require(pyrowave_encoder_encode_gpu_scaled(fixture.encoder, &acquire, &ownership,
		                                                      &scaling, &rate) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting invalid precision before GPU submission");
		scaling.intermediate_plane_format = VK_FORMAT_R8_UNORM;
		rate.maximum_bitstream_size = 0;
		require(pyrowave_encoder_encode_gpu_scaled(fixture.encoder, &acquire, &ownership,
		                                                      &scaling, &rate) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting inner-encoder budget after scaling");
		uint64_t completed = 0;
		require(device.get_device_table().vkGetSemaphoreCounterValue(device.get_device(),
		                                                           semaphore->get_semaphore(), &completed) == VK_SUCCESS,
		        "reading release completion");
		require(completed == 1, "error returned before queued scaling and external release completed");
		ownership.sync = {};
		rate.maximum_bitstream_size = 65536;
	}

	for (auto precision : {VK_FORMAT_R8_UNORM, VK_FORMAT_R8_UNORM, VK_FORMAT_R16_UNORM,
	                       VK_FORMAT_R16_UNORM, VK_FORMAT_R8_UNORM})
	{
		scaling.intermediate_plane_format = precision;
		scaling.output_color_space = precision == VK_FORMAT_R16_UNORM
		                            ? VK_COLOR_SPACE_HDR10_ST2084_EXT : VK_COLOR_SPACE_SRGB_NONLINEAR_KHR;
		scaling.ycbcr_chroma_midpoint = precision == VK_FORMAT_R16_UNORM ? 0.5f : 128.0f / 255.0f;
		require(pyrowave_encoder_encode_gpu_scaled(fixture.encoder, &ownership, &ownership,
		                                                      &scaling, &rate) == PYROWAVE_SUCCESS,
		        "encoding RGB image after configuration change or failure");
		size_t packets = 0;
		require(pyrowave_encoder_compute_num_packets(fixture.encoder, 65536, &packets) == PYROWAVE_SUCCESS,
		        "waiting for encoded frame");
		require(packets > 0, "scaled encoder produced no packets");
		for (auto &plane : fixture.encoder->scaler_planes)
			require(plane->get_format() == precision, "scaler retained the preceding intermediate precision");
		if (error_cleanup)
			break;
	}
}

static void test_decoder_record_progress()
{
	Fixture fixture;
	pyrowave_decoder_create_info info = {fixture.device, 128, 128, PYROWAVE_CHROMA_SUBSAMPLING_444, false};
	pyrowave_decoder decoder = nullptr;
	require(pyrowave_decoder_create(&info, &decoder) == PYROWAVE_SUCCESS, "creating decoder");
	BitstreamHeader header = {};
	header.payload_words = sizeof(header) / sizeof(uint32_t);
	require(pyrowave_decoder_push_packet(decoder, &header, sizeof(header)) == PYROWAVE_SUCCESS,
	        "queuing valid record");
	require(pyrowave_decoder_push_packet(decoder, &header, sizeof(header)) == PYROWAVE_SUCCESS,
	        "accepting valid duplicate record");
	for (unsigned words : {0u, 1u})
	{
		header.payload_words = words;
		require(pyrowave_decoder_push_packet(decoder, &header, sizeof(header)) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting duplicate record smaller than its header");
	}
	header.payload_words = sizeof(header) / sizeof(uint32_t);
	require(pyrowave_decoder_push_packet(decoder, &header, sizeof(header)) == PYROWAVE_SUCCESS,
	        "accepting valid duplicate after rejected record");
	pyrowave_decoder_destroy(decoder);
}

static PFN_vkCmdCopyBuffer native_copy_buffer;
static VkBuffer pool_copy_source;
static VkDeviceSize pool_copy_capacity;
static bool pool_copy_seen;
static bool pool_copy_valid;

static VKAPI_ATTR void VKAPI_CALL validate_pool_copy(VkCommandBuffer cmd, VkBuffer src, VkBuffer dst,
                                                   uint32_t count, const VkBufferCopy *regions)
{
	if (src == pool_copy_source)
	{
		pool_copy_seen = true;
		for (uint32_t i = 0; i < count; i++)
		{
			if (regions[i].srcOffset > pool_copy_capacity ||
			    regions[i].size > pool_copy_capacity - regions[i].srcOffset)
			{
				pool_copy_valid = false;
				return;
			}
		}
	}
	native_copy_buffer(cmd, src, dst, count, regions);
}

static void test_pool_growth_retry()
{
	Fixture fixture;
	std::vector<uint8_t> samples(128 * 128, 128);
	pyrowave_cpu_buffer input = {};
	input.width = input.height = 128;
	input.format = PYROWAVE_CPU_BUFFER_FORMAT_YUV444P;
	for (unsigned i = 0; i < 3; i++)
	{
		input.data[i] = samples.data();
		input.row_stride_in_bytes[i] = 128;
		input.plane_size_in_bytes[i] = samples.size();
	}
	pyrowave_rate_control rate = {65536};
	require(pyrowave_encoder_encode_cpu(fixture.encoder, &input, &rate) == PYROWAVE_SUCCESS,
	        "initializing pooled buffers");
	fixture.encoder->contexts[0].queued_fence->wait();
	// Simulate successful host growth followed by device allocation failure.
	auto larger_host = fixture.encoder->contexts[0].queued_bitstream->get_create_info();
	larger_host.size *= 2;
	fixture.encoder->contexts[0].queued_bitstream = fixture.device->device.create_buffer(larger_host);
	require(bool(fixture.encoder->contexts[0].queued_bitstream), "simulating interrupted pool growth");
	pool_copy_source = fixture.encoder->contexts[0].queued_bitstream_gpu->get_buffer();
	pool_copy_capacity = fixture.encoder->contexts[0].queued_bitstream_gpu->get_create_info().size;
	pool_copy_seen = false;
	pool_copy_valid = true;
	auto &table = const_cast<VolkDeviceTable &>(fixture.device->device.get_device_table());
	native_copy_buffer = table.vkCmdCopyBuffer;
	// Observe the production copy and block an invalid region before Vulkan.
	table.vkCmdCopyBuffer = validate_pool_copy;
	rate.maximum_bitstream_size /= 2;
	auto result = pyrowave_encoder_encode_cpu(fixture.encoder, &input, &rate);
	fixture.encoder->contexts[0].queued_fence->wait();
	table.vkCmdCopyBuffer = native_copy_buffer;
	require(result == PYROWAVE_SUCCESS, "retrying encode at a smaller budget");
	require(pool_copy_seen && pool_copy_valid, "pooled copy exceeded the retained GPU buffer");
}

int main(int argc, char **argv)
{
	try
	{
		require(argc == 1 || (argc == 2 && (std::strcmp(argv[1], "--error-cleanup") == 0 ||
		                                  std::strcmp(argv[1], "--precision") == 0 ||
		                                  std::strcmp(argv[1], "--contexts") == 0 ||
		                                  std::strcmp(argv[1], "--formats") == 0 ||
		                                  std::strcmp(argv[1], "--pool-retry") == 0)),
		        "usage: pyrowave-seam-test [--error-cleanup|--precision|--pool-retry|--contexts|--formats]");
		if (argc == 1 || std::strcmp(argv[1], "--contexts") == 0)
			test_frame_context_bounds();
		if (argc == 1 || std::strcmp(argv[1], "--formats") == 0)
			test_cpu_format_changes();
		if (argc == 1 || std::strcmp(argv[1], "--error-cleanup") == 0)
			test_scaled_encoding(true);
		if (argc == 1 || std::strcmp(argv[1], "--precision") == 0)
			test_scaled_encoding(false);
		if (argc == 1)
			test_decoder_record_progress();
		if (argc == 1 || std::strcmp(argv[1], "--pool-retry") == 0)
			test_pool_growth_retry();
		std::puts("Native seam regressions passed.");
		return 0;
	}
	catch (const std::exception &error)
	{
		std::fprintf(stderr, "%s\n", error.what());
		return 1;
	}
}
