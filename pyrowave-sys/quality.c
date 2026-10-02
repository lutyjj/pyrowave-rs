#include <pyrowave_regression_results.h>

/* The upstream model is a static function in a header; give it linkage. */
double pyrowave_quality_estimate_mbits(int psnr, int width, int height, int height_factor, int chroma444, double fps)
{
	return pyrowave_psnr_hvs_m_h_estimate_mbits(psnr, width, height, (enum pyrowave_height_factor)height_factor,
	                                            chroma444, fps);
}
