// Compile-only same-SDK declarations for the Rust audit host's public calls.
#import <Foundation/Foundation.h>
#include <dispatch/dispatch.h>
#include <stdbool.h>
#include <stdint.h>

_Static_assert(__builtin_types_compatible_p(BOOL, bool), "arm64 BOOL must be bool");
_Static_assert(__builtin_types_compatible_p(dispatch_time_t, uint64_t), "dispatch time type changed");
static dispatch_time_t (*const checked_time)(dispatch_time_t, int64_t) = dispatch_time;
static void (*const checked_after)(dispatch_time_t, dispatch_queue_t, void *, dispatch_function_t) = dispatch_after_f;

void captured_selection_public_declarations(id object) {
    NSHashTable<id> *table = [NSHashTable weakObjectsHashTable];
    [table addObject:object];
    id observed = table.anyObject;
    (void)observed;
    (void)checked_time;
    (void)checked_after;
}
