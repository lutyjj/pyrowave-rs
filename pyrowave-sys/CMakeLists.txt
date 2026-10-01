cmake_minimum_required(VERSION 3.20)
project(pyrowave-sys LANGUAGES CXX C)

find_program(PATCH_EXECUTABLE patch REQUIRED)
find_package(Git QUIET)
set(UPSTREAM_DIR ${CMAKE_CURRENT_SOURCE_DIR}/upstream)
if(NOT EXISTS ${UPSTREAM_DIR}/pyrowave/pyrowave.h OR
   NOT EXISTS ${UPSTREAM_DIR}/Granite/third_party/volk/volk.h OR
   NOT EXISTS ${UPSTREAM_DIR}/Granite/third_party/khronos/vulkan-headers/include/vulkan/vulkan.h)
	message(FATAL_ERROR "Initialize the upstream submodules as described in pyrowave-sys/README.md")
endif()

# Apply compatibility patches to an owned build copy, keeping Git links clean.
set(PYROWAVE_VENDOR_DIR ${CMAKE_CURRENT_BINARY_DIR}/vendor/pyrowave)
file(REMOVE_RECURSE ${PYROWAVE_VENDOR_DIR})
file(COPY ${UPSTREAM_DIR}/pyrowave/ DESTINATION ${PYROWAVE_VENDOR_DIR}
	PATTERN ".git" EXCLUDE PATTERN ".github" EXCLUDE)
file(COPY ${UPSTREAM_DIR}/Granite/ DESTINATION ${PYROWAVE_VENDOR_DIR}/Granite
	PATTERN ".git" EXCLUDE PATTERN ".github" EXCLUDE)
configure_file(THIRD-PARTY-NOTICES ${PYROWAVE_VENDOR_DIR}/THIRD-PARTY-NOTICES COPYONLY)
file(GLOB LOCAL_PATCHES CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/patches/*.patch)
foreach(PATCH IN LISTS LOCAL_PATCHES)
	execute_process(
		COMMAND ${PATCH_EXECUTABLE} --batch --forward -p1 -i ${PATCH}
		WORKING_DIRECTORY ${PYROWAVE_VENDOR_DIR}
		COMMAND_ERROR_IS_FATAL ANY)
endforeach()

set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/UPSTREAM_REVISIONS)
file(STRINGS ${CMAKE_CURRENT_SOURCE_DIR}/UPSTREAM_REVISIONS REVISIONS)
foreach(ENTRY IN LISTS REVISIONS)
	if(NOT ENTRY MATCHES "^([^:]+): ([0-9a-f]+)$")
		message(FATAL_ERROR "Invalid upstream revision: ${ENTRY}")
	endif()
	set(SOURCE ${CMAKE_MATCH_1})
	set(REVISION ${CMAKE_MATCH_2})
	string(LENGTH "${REVISION}" REVISION_LENGTH)
	if(NOT REVISION_LENGTH EQUAL 40)
		message(FATAL_ERROR "Invalid upstream revision: ${ENTRY}")
	endif()
	if(EXISTS ${UPSTREAM_DIR}/${SOURCE}/.git AND Git_FOUND)
		execute_process(COMMAND ${GIT_EXECUTABLE} -C ${UPSTREAM_DIR}/${SOURCE} rev-parse HEAD
			OUTPUT_VARIABLE ACTUAL OUTPUT_STRIP_TRAILING_WHITESPACE COMMAND_ERROR_IS_FATAL ANY)
		execute_process(COMMAND ${GIT_EXECUTABLE} -C ${UPSTREAM_DIR}/${SOURCE} rev-parse --absolute-git-dir
			OUTPUT_VARIABLE GIT_DIR OUTPUT_STRIP_TRAILING_WHITESPACE COMMAND_ERROR_IS_FATAL ANY)
		file(GLOB_RECURSE GIT_REFS CONFIGURE_DEPENDS ${GIT_DIR}/refs/*)
		set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${GIT_DIR}/HEAD ${GIT_REFS})
		if(EXISTS ${GIT_DIR}/packed-refs)
			set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${GIT_DIR}/packed-refs)
		endif()
		if(NOT ACTUAL STREQUAL REVISION)
			message(FATAL_ERROR "Update UPSTREAM_REVISIONS to match ${SOURCE}: ${ACTUAL}")
		endif()
	endif()
	string(APPEND PROVENANCE "${SOURCE}: ${REVISION}\n")
endforeach()
file(WRITE ${PYROWAVE_VENDOR_DIR}/VENDOR.txt "${PROVENANCE}")

# Granite is only the Vulkan backend PyroWave is written against: no renderer,
# windowing, runtime shader compiler, FFmpeg or audio.
set(GRANITE_RENDERER OFF CACHE BOOL "" FORCE)
set(GRANITE_VULKAN_SPIRV_CROSS OFF CACHE BOOL "" FORCE)
set(GRANITE_VULKAN_SHADER_MANAGER_RUNTIME_COMPILER OFF CACHE BOOL "" FORCE)
set(GRANITE_VULKAN_SYSTEM_HANDLES OFF CACHE BOOL "" FORCE)
set(GRANITE_VULKAN_FOSSILIZE OFF CACHE BOOL "" FORCE)
set(GRANITE_SHADER_COMPILER_OPTIMIZE OFF CACHE BOOL "" FORCE)
set(GRANITE_POSITION_INDEPENDENT ON CACHE BOOL "" FORCE)
set(GRANITE_SHIPPING ON CACHE BOOL "" FORCE)
set(GRANITE_TOOLS OFF CACHE BOOL "" FORCE)
set(GRANITE_FFMPEG OFF CACHE BOOL "" FORCE)
set(GRANITE_AUDIO OFF CACHE BOOL "" FORCE)
set(GRANITE_BULLET OFF CACHE BOOL "" FORCE)
set(GRANITE_HIDDEN ON CACHE BOOL "" FORCE)
set(GRANITE_PLATFORM "null" CACHE STRING "" FORCE)

add_subdirectory(${PYROWAVE_VENDOR_DIR}/Granite ${CMAKE_CURRENT_BINARY_DIR}/Granite EXCLUDE_FROM_ALL)

# volk defines every vk* entry point as a global function pointer. Namespace it
# so those symbols can never bind calls made by other Vulkan users in the
# process.
set_source_files_properties(${PYROWAVE_VENDOR_DIR}/Granite/third_party/volk/volk.c
	TARGET_DIRECTORY granite-volk PROPERTIES LANGUAGE CXX)
target_compile_definitions(granite-volk-headers INTERFACE VOLK_NAMESPACE)

# Not the top-level project, so upstream only defines the static `pyrowave` codec.
add_subdirectory(${PYROWAVE_VENDOR_DIR} ${CMAKE_CURRENT_BINARY_DIR}/pyrowave EXCLUDE_FROM_ALL)

# Upstream ships the C API only as a shared library; build it static instead.
# Granite's scaler backs pyrowave_encoder_encode_gpu_scaled_synchronous().
add_library(pyrowave-capi STATIC
	${PYROWAVE_VENDOR_DIR}/pyrowave_c.cpp
	${PYROWAVE_VENDOR_DIR}/Granite/video/scaler.cpp)
target_include_directories(pyrowave-capi PRIVATE
	${PYROWAVE_VENDOR_DIR}/shaders
	${PYROWAVE_VENDOR_DIR}/Granite/video)
target_link_libraries(pyrowave-capi PRIVATE pyrowave granite-vulkan granite-math)
target_compile_options(pyrowave-capi PRIVATE -fvisibility=hidden -Wno-missing-field-initializers)
set_target_properties(pyrowave-capi PROPERTIES POSITION_INDEPENDENT_CODE ON)

# Upstream's GPU roundtrip and error-handling oracle, built only on request.
add_executable(pyrowave-c-test EXCLUDE_FROM_ALL ${PYROWAVE_VENDOR_DIR}/pyrowave_c_test.cpp)
target_link_libraries(pyrowave-c-test PRIVATE pyrowave-capi granite-volk-headers)

add_executable(pyrowave-seam-test EXCLUDE_FROM_ALL tests/native.cpp)
target_include_directories(pyrowave-seam-test PRIVATE
	${PYROWAVE_VENDOR_DIR}
	${PYROWAVE_VENDOR_DIR}/shaders
	${PYROWAVE_VENDOR_DIR}/Granite/video)
target_link_libraries(pyrowave-seam-test PRIVATE pyrowave-capi pyrowave granite-vulkan granite-math)

add_executable(pyrowave-decode EXCLUDE_FROM_ALL
	${PYROWAVE_VENDOR_DIR}/decode.cpp
	${PYROWAVE_VENDOR_DIR}/yuv4mpeg.cpp)
target_include_directories(pyrowave-decode PRIVATE ${PYROWAVE_VENDOR_DIR}/Granite/application/global)
target_link_libraries(pyrowave-decode PRIVATE pyrowave granite-vulkan granite-math)
