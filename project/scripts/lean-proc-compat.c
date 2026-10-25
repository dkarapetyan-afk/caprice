/* Optional host compatibility only: some containers expose /proc/self/exe but
   not /proc/<getpid()>/exe. This leaves the Lean kernel and its inputs intact. */
#define _GNU_SOURCE
#include <unistd.h>
#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
ssize_t readlink(const char *path, char *buf, size_t size) {
    ssize_t (*original)(const char*,char*,size_t) = dlsym(RTLD_NEXT,"readlink");
    char self[64]; snprintf(self,sizeof(self),"/proc/%d/exe",getpid());
    return original(strcmp(path,self)==0 ? "/proc/self/exe" : path,buf,size);
}
