#include <stdio.h>
#include <stdlib.h>

int main() {
    const char *p = getenv("ZCORE_PERSONALITY");
    if (p) {
        printf("personality: %s\n", p);
    } else {
        printf("personality: unknown (ZCORE_PERSONALITY not set)\n");
    }
    return 0;
}
