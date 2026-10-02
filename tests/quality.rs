use pyrowave::{ChromaSampling, QUALITY_RANGE, QualityTarget, VideoFormat, estimate_bitrate};

fn format(width: u32, height: u32, chroma: ChromaSampling) -> VideoFormat {
	VideoFormat { width, height, chroma }
}

fn target(psnr_hvs_m_h: u32) -> QualityTarget {
	QualityTarget {
		psnr_hvs_m_h,
		viewing_distance: 2.0,
	}
}

#[test]
fn estimate_matches_upstream_evaluation() {
	// Upstream's write-up: 720p to 4K at 60 fps spans about 125 to 300 Mbit/s
	// for the 35 dB curve at two screen heights.
	let at = |width, height| estimate_bitrate(format(width, height, ChromaSampling::Yuv420), 60.0, target(35)).unwrap();
	assert!((120e6..150e6).contains(&at(1280, 720)));
	assert!((280e6..310e6).contains(&at(3840, 2160)));
}

#[test]
fn estimate_scales_with_frame_rate_quality_and_chroma() {
	let uhd = format(3840, 2160, ChromaSampling::Yuv420);
	let base = estimate_bitrate(uhd, 60.0, target(35)).unwrap();
	assert_eq!(estimate_bitrate(uhd, 120.0, target(35)).unwrap(), base * 2.0);
	assert!(estimate_bitrate(uhd, 60.0, target(40)).unwrap() > base);
	assert!(estimate_bitrate(format(3840, 2160, ChromaSampling::Yuv444), 60.0, target(35)).unwrap() > base);
	let closer = QualityTarget {
		viewing_distance: 1.0,
		..target(35)
	};
	assert!(estimate_bitrate(uhd, 60.0, closer).unwrap() > base);
}

#[test]
fn estimate_rejects_inputs_outside_the_model() {
	let uhd = format(3840, 2160, ChromaSampling::Yuv420);
	assert!(estimate_bitrate(format(640, 360, ChromaSampling::Yuv420), 60.0, target(35)).is_err());
	assert!(estimate_bitrate(format(7680, 4320, ChromaSampling::Yuv420), 60.0, target(35)).is_err());
	assert!(estimate_bitrate(uhd, 60.0, target(QUALITY_RANGE.start() - 1)).is_err());
	assert!(estimate_bitrate(uhd, 60.0, target(QUALITY_RANGE.end() + 1)).is_err());
}

#[test]
fn estimate_rejects_invalid_frame_rates_and_distances() {
	let uhd = format(3840, 2160, ChromaSampling::Yuv420);
	for fps in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
		assert!(estimate_bitrate(uhd, fps, target(35)).is_err());
	}
	for viewing_distance in [0.0, 2.876, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
		assert!(
			estimate_bitrate(
				uhd,
				60.0,
				QualityTarget {
					viewing_distance,
					..target(35)
				}
			)
			.is_err()
		);
	}
}
