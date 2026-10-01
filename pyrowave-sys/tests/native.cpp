#include <cstdio>
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

	Fixture()
	{
		require(pyrowave_create_default_device(&device) == PYROWAVE_SUCCESS, "creating Vulkan device");
		pyrowave_encoder_create_info info = {device, 128, 128, PYROWAVE_CHROMA_SUBSAMPLING_444};
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
		require(pyrowave_encoder_encode_gpu_scaled_synchronous(fixture.encoder, &acquire, &ownership,
		                                                      &scaling, &rate) == PYROWAVE_ERROR_INVALID_ARGUMENT,
		        "rejecting invalid precision before GPU submission");
		scaling.intermediate_plane_format = VK_FORMAT_R8_UNORM;
		rate.maximum_bitstream_size = 0;
		require(pyrowave_encoder_encode_gpu_scaled_synchronous(fixture.encoder, &acquire, &ownership,
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
		require(pyrowave_encoder_encode_gpu_scaled_synchronous(fixture.encoder, &ownership, &ownership,
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
	require(pyrowave_encoder_encode_cpu_synchronous(fixture.encoder, &input, &rate) == PYROWAVE_SUCCESS,
	        "initializing pooled buffers");
	fixture.encoder->queued_fence->wait();
	// Simulate successful host growth followed by device allocation failure.
	auto larger_host = fixture.encoder->queued_bitstream->get_create_info();
	larger_host.size *= 2;
	fixture.encoder->queued_bitstream = fixture.device->device.create_buffer(larger_host);
	require(bool(fixture.encoder->queued_bitstream), "simulating interrupted pool growth");
	pool_copy_source = fixture.encoder->queued_bitstream_gpu->get_buffer();
	pool_copy_capacity = fixture.encoder->queued_bitstream_gpu->get_create_info().size;
	pool_copy_seen = false;
	pool_copy_valid = true;
	auto &table = const_cast<VolkDeviceTable &>(fixture.device->device.get_device_table());
	native_copy_buffer = table.vkCmdCopyBuffer;
	// Observe the production copy and block an invalid region before Vulkan.
	table.vkCmdCopyBuffer = validate_pool_copy;
	rate.maximum_bitstream_size /= 2;
	auto result = pyrowave_encoder_encode_cpu_synchronous(fixture.encoder, &input, &rate);
	fixture.encoder->queued_fence->wait();
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
		                                  std::strcmp(argv[1], "--pool-retry") == 0)),
		        "usage: pyrowave-seam-test [--error-cleanup|--precision|--pool-retry]");
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
