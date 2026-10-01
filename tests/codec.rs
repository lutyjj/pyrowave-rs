use pyrowave::{ChromaSampling, Decoder, Device, Encoder, VideoFormat};

#[test]
#[ignore = "requires a compatible Vulkan GPU"]
fn roundtrip_retains_device_and_accepts_unaligned_packets() {
	for chroma in [ChromaSampling::Yuv420, ChromaSampling::Yuv444] {
		let format = VideoFormat {
			width: 256,
			height: 128,
			chroma,
		};
		let device = Device::new().unwrap();
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
		for luma in [32, 96, 192] {
			let input = [vec![luma; pixels], vec![112; chroma_pixels], vec![140; chroma_pixels]];
			for invalid_budget in [0, 4, 7] {
				assert!(matches!(
					encoder.encode_cpu(input.each_ref().map(Vec::as_slice), invalid_budget),
					Err(pyrowave::Error::InvalidInput(_))
				));
			}
			let encoded = encoder.encode_cpu(input.each_ref().map(Vec::as_slice), budget).unwrap();
			assert!(encoded.len() <= budget, "native rate control exceeded its budget");
			let mut unaligned = vec![0];
			unaligned.extend_from_slice(encoded);
			decoder.push_packet(&unaligned[1..]).unwrap();
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
		assert!(encoder.encode_cpu([&[]; 3], budget).is_err());
		assert!(decoder.decode_cpu([&mut [], &mut [], &mut []]).is_err());
		assert!(decoder.push_packet(&[0; 3]).is_err());
		decoder.clear();
		assert!(!decoder.is_ready(false));
	}
}

#[test]
#[ignore = "requires a compatible Vulkan GPU"]
fn rejects_truncated_control_coefficients_and_signs() {
	let mut decoder = Decoder::new(
		Device::new().unwrap(),
		VideoFormat {
			width: 128,
			height: 128,
			chroma: ChromaSampling::Yuv444,
		},
	)
	.unwrap();
	// Native little-endian records: ballot, word count/sequence, quantizer/index,
	// then two control bytes and one quantizer byte per populated 8x8 block.
	let truncated: &[&[u8]] = &[
		&[0xff, 0xff, 2, 0, 0, 0, 0, 0],
		&[1, 0, 3, 0, 0, 0, 0, 0, 3, 0, 0, 0xff],
		&[1, 0, 3, 0, 0, 0, 0, 0, 1, 0, 0, 0xff],
	];
	for packet in truncated {
		decoder.clear();
		assert!(decoder.push_packet(packet).is_err());
	}
	let valid = [1, 0, 4, 0, 0, 0, 0, 0, 1, 0, 0, 0xff, 0, 0, 0, 0];
	decoder.push_packet(&valid).unwrap();
	// Validation still applies when the block is already present.
	for packet in truncated {
		assert!(decoder.push_packet(packet).is_err());
	}
}
