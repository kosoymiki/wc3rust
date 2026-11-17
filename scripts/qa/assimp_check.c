// Minimal assimp-backed glTF checker for QA.
//
// The Termux `assimp` package ships libassimp.so without a CLI, so this tiny
// consumer links the system library and reports what assimp actually imports:
// mesh count, vertices, faces and materials. It is a validation probe, not a
// renderer.
//
// Build: clang -O2 assimp_check.c -o assimp_check -lassimp
// Usage: assimp_check model.gltf

#include <stdio.h>

#include <assimp/cimport.h>
#include <assimp/postprocess.h>
#include <assimp/scene.h>

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: assimp_check <model.gltf>\n");
        return 2;
    }
    const struct aiScene *scene = aiImportFile(
        argv[1], aiProcess_Triangulate | aiProcess_JoinIdenticalVertices);
    if (scene == NULL) {
        fprintf(stderr, "ASSIMP_FAIL %s: %s\n", argv[1], aiGetErrorString());
        return 1;
    }
    unsigned long long vertices = 0;
    unsigned long long faces = 0;
    for (unsigned int mesh = 0; mesh < scene->mNumMeshes; ++mesh) {
        const struct aiMesh *m = scene->mMeshes[mesh];
        vertices += m->mNumVertices;
        faces += m->mNumFaces;
    }
    printf(
        "ASSIMP_OK meshes=%u vertices=%llu faces=%llu materials=%u\n",
        scene->mNumMeshes,
        vertices,
        faces,
        scene->mNumMaterials);
    aiReleaseImport(scene);
    return 0;
}
