rm *.spv
glslangValidator.exe .\tri.vert.glsl -V --target-env vulkan1.0 -o tri.vert.spv
glslangValidator.exe .\tri.frag.glsl -V --target-env vulkan1.0 -o tri.frag.spv
