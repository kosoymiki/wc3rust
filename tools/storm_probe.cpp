// StormLib probe: list and selectively extract MPQ entries.
// Build: g++ -std=c++20 -I donors/StormLib/src tools/storm_probe.cpp \
//        third_party/stormlib-build2/libstorm.a -lz -lbz2 -ldl -lpthread -o tools/storm_probe
#include <StormLib.h>

#include <cstdio>
#include <cstring>
#include <filesystem>
#include <string>
#include <vector>

namespace fs = std::filesystem;

static std::string sanitize(const char* name) {
    std::string s(name ? name : "");
    for (char& c : s) {
        if (c == '\\') c = '/';
        if (c == ':' || c == '*' || c == '?' || c == '"' || c == '<' || c == '>' || c == '|') c = '_';
    }
    while (!s.empty() && (s.front() == '/' || s.front() == '.')) s.erase(s.begin());
    return s;
}

static bool extract_one(HANDLE mpq, const char* name, const fs::path& outdir) {
    HANDLE file = nullptr;
    if (!SFileOpenFileEx(mpq, name, SFILE_OPEN_FROM_MPQ, &file)) {
        std::fprintf(stderr, "open failed %s: %u\n", name, SErrGetLastError());
        return false;
    }
    DWORD size = SFileGetFileSize(file, nullptr);
    if (size == SFILE_INVALID_SIZE) {
        std::fprintf(stderr, "size failed %s\n", name);
        SFileCloseFile(file);
        return false;
    }
    std::vector<unsigned char> data(size);
    DWORD read = 0;
    bool ok = SFileReadFile(file, data.data(), size, &read, nullptr) != 0;
    SFileCloseFile(file);
    if (!ok || read != size) {
        std::fprintf(stderr, "read failed %s (%lu/%lu)\n", name, (unsigned long)read, (unsigned long)size);
        return false;
    }
    fs::path out = outdir / sanitize(name);
    fs::create_directories(out.parent_path());
    FILE* fh = std::fopen(out.string().c_str(), "wb");
    if (!fh) {
        std::fprintf(stderr, "write failed %s\n", out.string().c_str());
        return false;
    }
    std::fwrite(data.data(), 1, data.size(), fh);
    std::fclose(fh);
    std::printf("EXTRACT %s %lu %s\n", name, (unsigned long)size, out.string().c_str());
    return true;
}

int main(int argc, char** argv) {
    if (argc < 2) {
        std::fprintf(stderr, "usage: %s <archive.mpq> [--extract <substr> <outdir>] [--limit N]\n", argv[0]);
        return 2;
    }
    const char* archive = argv[1];
    const char* filter = nullptr;
    const char* outdir = nullptr;
    int limit = 1000000;
    for (int i = 2; i < argc; ++i) {
        if (std::strcmp(argv[i], "--extract") == 0 && i + 2 < argc) {
            filter = argv[++i];
            outdir = argv[++i];
        } else if (std::strcmp(argv[i], "--limit") == 0 && i + 1 < argc) {
            limit = std::atoi(argv[++i]);
        }
    }

    HANDLE mpq = nullptr;
    if (!SFileOpenArchive(archive, 0, MPQ_OPEN_READ_ONLY, &mpq)) {
        std::fprintf(stderr, "SFileOpenArchive failed: %u\n", SErrGetLastError());
        return 1;
    }
    std::printf("ARCHIVE %s\n", archive);
    SFILE_FIND_DATA fd{};
    HANDLE find = SFileFindFirstFile(mpq, "*", &fd, nullptr);
    if (find == nullptr) {
        std::fprintf(stderr, "SFileFindFirstFile failed: %u\n", SErrGetLastError());
        SFileCloseArchive(mpq);
        return 1;
    }
    int listed = 0;
    int extracted = 0;
    do {
        if (listed < limit) {
            std::printf("FILE %s %lu\n", fd.cFileName, (unsigned long)fd.dwFileSize);
        }
        if (filter && std::strstr(fd.cFileName, filter) != nullptr && outdir != nullptr) {
            if (extract_one(mpq, fd.cFileName, outdir)) ++extracted;
        }
        ++listed;
    } while (SFileFindNextFile(find, &fd));
    SFileFindClose(find);
    SFileCloseArchive(mpq);
    std::printf("SUMMARY listed=%d extracted=%d\n", listed, extracted);
    return extracted > 0 || filter == nullptr ? 0 : 1;
}
