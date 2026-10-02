/* pyrowave.h refuses to compile without the Vulkan headers included first. */
#include <vulkan/vulkan.h>
#include <pyrowave.h>

/* The author's bitrate model, with the entry point quality.c gives linkage. */
#include <eval-results/pyrowave_regression_results.h>
double pyrowave_quality_estimate_mbits(int psnr, int width, int height, int height_factor, int chroma444, double fps);
