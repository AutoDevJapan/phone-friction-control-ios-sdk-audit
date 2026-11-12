// SDK inspection host only. Invoke the borrowed callback synchronously once;
// never store either pointer. The compiler owns the autorelease-pool boundary.
void audit_capture_autorelease_pool(void (*body)(void *), void *context) {
    if (body == 0 || context == 0) {
        __builtin_trap();
    }
    @autoreleasepool {
        body(context);
    }
}
