use pyrowave::{ChromaSampling, Decoder, Device, Encoder, VideoFormat, sys, with_native};

#[test]
#[ignore = "requires a compatible Vulkan GPU"]
fn roundtrip_retains_device_and_accepts_unaligned_packets() {
	for chroma in [ChromaSampling::Yuv420, ChromaSampling::Yuv444] {
		let format = VideoFormat {
			width: 256,
			height: 128,
			chroma,
		};
		let mut raw_device = std::ptr::null_mut();
		// SAFETY: the output is live and ownership transfers to Device below.
		assert_eq!(
			with_native(|| unsafe { sys::pyrowave_create_default_device(&mut raw_device) }),
			sys::pyrowave_result_PYROWAVE_SUCCESS
		);
		// SAFETY: this successful creation has no other owner or borrowed Vulkan resources.
		let device = unsafe { Device::from_raw(raw_device) };
		let mut encoder = Encoder::new(device.clone(), format).unwrap();
		let mut decoder = Decoder::new(device.clone(), format).unwrap();
		drop(device);
		let pixels = (format.width * format.height) as usize;
		let chroma_pixels = if chroma == ChromaSampling::Yuv420 {
			pixels / 4
		} else {
			pixels
		};
		let budget = pixels * 2;
		for (frame, luma) in [32, 96, 192].into_iter().enumerate() {
			// SAFETY: the retained encoder has completed the preceding frame.
			assert_eq!(
				with_native(|| unsafe {
					sys::pyrowave_encoder_set_frame_context(encoder.as_raw(), (frame % 2) as i32)
				}),
				sys::pyrowave_result_PYROWAVE_SUCCESS
			);
			let input = [vec![luma; pixels], vec![112; chroma_pixels], vec![140; chroma_pixels]];
			let encoded = encoder.encode_cpu(input.each_ref().map(Vec::as_slice), budget).unwrap();
			assert!(encoded.len() <= budget, "native rate control exceeded its budget");
			let mut unaligned = vec![0];
			unaligned.extend_from_slice(encoded);
			// SAFETY: the records came from the linked encoder with the same format.
			unsafe { decoder.push_packet(&unaligned[1..]) }.unwrap();
			assert!(decoder.is_ready(false));
			let mut output = [
				vec![0xa5; pixels + 64],
				vec![0xa5; chroma_pixels + 64],
				vec![0xa5; chroma_pixels + 64],
			];
			decoder.decode_cpu(output.each_mut().map(Vec::as_mut_slice)).unwrap();
			for (plane, expected) in output.iter().zip([luma, 112, 140]) {
				let (samples, tail) = plane.split_at(plane.len() - 64);
				assert!(samples.iter().all(|sample| sample.abs_diff(expected) <= 2));
				assert_eq!(tail, &[0xa5; 64]);
			}
		}
		let report = encoder.device().performance_report(true);
		assert!(report.iter().any(|line| line.contains("ms per frame")));
		assert!(encoder.encode_cpu([&[]; 3], budget).is_err());
		assert!(decoder.decode_cpu([&mut [], &mut [], &mut []]).is_err());
		// SAFETY: no records are supplied; upstream treats empty input as a no-op.
		unsafe { decoder.push_packet(&[]) }.unwrap();
		decoder.clear();
		assert!(!decoder.is_ready(false));
	}
}

#[test]
#[ignore = "requires a compatible Vulkan GPU"]
fn cpu_budget_results_match_upstream() {
	let format = VideoFormat {
		width: 128,
		height: 128,
		chroma: ChromaSampling::Yuv420,
	};
	let device = Device::new().unwrap();
	let mut encoder = Encoder::new(device.clone(), format).unwrap();
	let native = Encoder::new(device, format).unwrap();
	let input = [vec![96u8; 128 * 128], vec![128; 64 * 64], vec![128; 64 * 64]];
	let buffers = sys::pyrowave_cpu_buffer {
		data: input.each_ref().map(|plane| plane.as_ptr().cast_mut().cast()),
		row_stride_in_bytes: [128, 64, 64],
		plane_size_in_bytes: [128 * 128, 64 * 64, 64 * 64],
		width: 128,
		height: 128,
		format: sys::pyrowave_cpu_buffer_format_PYROWAVE_CPU_BUFFER_FORMAT_YUV420P,
	};
	for budget in [0, 3, 4, 7, 8, 32768] {
		let rate = sys::pyrowave_rate_control {
			maximum_bitstream_size: budget,
		};
		// SAFETY: the fixed eight-bit planes cover their layouts and stay live.
		let expected = with_native(|| unsafe { sys::pyrowave_encoder_encode_cpu(native.as_raw(), &buffers, &rate) });
		if expected == sys::pyrowave_result_PYROWAVE_SUCCESS {
			let mut packets = 0;
			// SAFETY: readback follows a successful encode and waits for completion.
			assert_eq!(
				with_native(|| unsafe {
					sys::pyrowave_encoder_compute_num_packets(native.as_raw(), usize::MAX / 2, &mut packets)
				}),
				sys::pyrowave_result_PYROWAVE_SUCCESS
			);
		}
		let actual = encoder.encode_cpu(input.each_ref().map(Vec::as_slice), budget);
		if expected == sys::pyrowave_result_PYROWAVE_SUCCESS {
			assert!(actual.is_ok(), "budget {budget}: {actual:?}");
		} else {
			assert!(
				matches!(actual, Err(pyrowave::Error::Native { code, .. }) if code == expected),
				"budget {budget}: {actual:?}, upstream returned {expected}"
			);
		}
	}
}
