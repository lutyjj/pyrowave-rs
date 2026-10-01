//! Encode and decode a constant eight-bit frame using the official C API.
use pyrowave::{ChromaSampling, Decoder, Device, Encoder, VideoFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let format = VideoFormat {
		width: 256,
		height: 128,
		chroma: ChromaSampling::Yuv444,
	};
	let device = Device::new()?;
	let mut encoder = Encoder::new(device.clone(), format)?;
	let mut decoder = Decoder::new(device, format)?;
	let pixels = (format.width * format.height) as usize;
	let input = [vec![96; pixels], vec![128; pixels], vec![128; pixels]];
	let encoded = encoder.encode_cpu(input.each_ref().map(Vec::as_slice), pixels * 2)?;
	println!("PyroWave {}: encoded {} bytes", pyrowave::BITSTREAM_ID, encoded.len());
	decoder.push_packet(encoded)?;
	if !decoder.is_ready(false) {
		return Err("encoded frame is incomplete".into());
	}
	let mut output = [vec![0; pixels], vec![0; pixels], vec![0; pixels]];
	decoder.decode_cpu(output.each_mut().map(Vec::as_mut_slice))?;
	let maximum_error = input
		.iter()
		.zip(&output)
		.flat_map(|(input, output)| input.iter().zip(output).map(|(a, b)| a.abs_diff(*b)))
		.max()
		.unwrap_or(0);
	println!(
		"Decoded {}x{}, maximum sample error: {maximum_error}",
		format.width, format.height
	);
	Ok(())
}
