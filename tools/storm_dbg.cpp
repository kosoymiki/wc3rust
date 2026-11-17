// StormLib diagnostic: print flags/offsets and the raw first bytes of one entry.
// Build: g++ -std=c++20 -I donors/StormLib/src tools/storm_dbg.cpp \
//        third_party/stormlib-build2/libstorm.a -lz -lbz2 -ldl -lpthread -o tools/storm_dbg
#include <StormLib.h>

#include <cstdio>
#include <cstring>
#include <vector>

int main(int argc, char** argv) {
    if (argc < 3) {
        std::fprintf(stderr, "usage: %s <archive> <name>\n", argv[0]);
        return 2;
    }
    HANDLE mpq = nullptr;
    if (!SFileOpenArchive(argv[1], 0, 0, &mpq)) {
        std::fprintf(stderr, "open archive failed: %u\n", SErrGetLastError());
        return 1;
    }
    HANDLE f = nullptr;
    if (!SFileOpenFileEx(mpq, argv[2], SFILE_OPEN_FROM_MPQ, &f)) {
        std::fprintf(stderr, "open file failed: %u\n", SErrGetLastError());
        return 1;
    }
    DWORD flags = 0, csize = 0, size = 0, offset = 0, sizehigh = 0;
    SFileGetFileInfo(f, SFileInfoFlags, &flags, sizeof(flags), nullptr);
    SFileGetFileInfo(f, SFileInfoCompressedSize, &csize, sizeof(csize), nullptr);
    SFileGetFileInfo(f, SFileInfoFileSize, &size, sizeof(size), nullptr);
    SFileGetFileInfo(f, SFileInfoByteOffset, &offset, sizeof(offset), nullptr);
    std::printf("flags=0x%08x csize=%u size=%u offset=%u\n", flags, csize, size, offset);
    std::vector<unsigned char> data(size);
    DWORD read = 0;
    bool ok = SFileReadFile(f, data.data(), size, &read, nullptr) != 0;
    std::printf("read=%d bytes=%u head=", ok ? 1 : 0, read);
    for (unsigned i = 0; i < 16 && i < read; i++) std::printf("%02x", data[i]);
    std::printf("\n");
    SFileCloseFile(f);
    SFileCloseArchive(mpq);
    return 0;
}
