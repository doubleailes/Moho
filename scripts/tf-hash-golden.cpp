// Golden values for Moho's port of `TfHash::Combine` (crates/moho/src/tf_hash.rs).
//
// Build against an OpenUSD build (headers and libusd_tf, libusd_arch), e.g.:
//   g++ -std=c++17 -I$USD/include tf-hash-golden.cpp -L$USD/lib \
//       -lusd_tf -lusd_arch -Wl,-rpath,$USD/lib -o tf-hash-golden
// (link libpython too when the build has Python support).
#include "pxr/base/tf/hash.h"

#include <cstdio>
#include <map>
#include <string>

PXR_NAMESPACE_USING_DIRECTIVE

int main() {
    const char *pairs[][2] = {
        {"/World/Instancer", "/"},
        {"/Tree/Leaves", "/Trees/tree/ForInstancer1"},
        {"", ""},
        {"/a", "/"},
        {"/World/sets/Rocks/PointInstancer_with_a_long_name_for_testing_the_32_byte_blocks", "/"},
        {"/Some/Very/Long/Path/That/Goes/On/And/On/For/Quite/A/While/To/Exceed/The/Spooky/Short/"
         "Hash/Threshold/Of/One/Hundred/And/Ninety/Two/Bytes/In/Length/For/Sure/Yes/Indeed/More/"
         "Than/Enough",
         "/Another/Prototype/Root/ForInstancer123abc"},
        {"/world/trees", "/"},
        {"/world/trees/Prototypes/tree/leaves",
         "/world/trees/Prototypes/tree/ForInstancer27d4476d2fc5f07a"},
    };
    // Messages of 192 bytes and more take SpookyHash's long path.
    const std::string long192 = "/" + std::string(191, 'a');
    const std::string long300 = [] { std::string s; for (int i = 0; i < 30; i++) s += "/segment_" + std::to_string(i); return s; }();
    printf("Combine(long192, \"/\") = %zx\n", TfHash::Combine(long192, std::string("/")));
    printf("Combine(long300, long192) = %zx\n", TfHash::Combine(long300, long192));
    for (auto &p : pairs) {
        // As piPrototypePropagatingSceneIndex.cpp's _InstancerHash.
        const size_t h = TfHash::Combine(std::string(p[0]), std::string(p[1]));
        printf("Combine(\"%s\", \"%s\") = %zx\n", p[0], p[1], h);
    }
    // As niInstanceAggregationSceneIndex.cpp's _ComputeConstantPrimvarsRoleHash.
    const std::map<std::string, std::string> maps[] = {
        {{"displayColor", "Color"}},
        {{"displayColor", "Color"}, {"displayOpacity", ""}, {"st", "TextureCoordinate"}},
        {{"foo", ""}},
    };
    for (auto &m : maps) {
        printf("Combine(map of %zu) = %zx\n", m.size(), TfHash::Combine(m));
    }
}
