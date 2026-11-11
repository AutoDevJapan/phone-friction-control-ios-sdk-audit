// Audit-host category observer. CATEGORY_TOKEN_COUNTS=1 additionally requires
// the host's public category-token count after the real checkbox operation.
#import <XCTest/XCTest.h>
#include <stdint.h>
#include <stdio.h>

#if CAPTURED_SELECTION_TEARDOWN && !CATEGORY_TOKEN_COUNTS
#error Captured Selection teardown requires public nonempty category-token counts
#endif

static void keepImage(XCTestCase *test, XCUIApplication *app, NSString *name) {
    XCTAttachment *a = [XCTAttachment attachmentWithScreenshot:app.screenshot];
    a.name = name;
    a.lifetime = XCTAttachmentLifetimeKeepAlways;
    [test addAttachment:a];
}
static bool snapshot(XCUIElement *badge, unsigned long *setters,
                     unsigned long *apps, unsigned long *tokens) {
    unsigned long initial = 0;
    int consumed = 0;
    const char *text = badge.label.UTF8String;
#if CATEGORY_TOKEN_COUNTS
    unsigned long categoryInitial = 0;
    return text && sscanf(text,
        "Selection snapshot setters=%lu initial=%lu apps=%lu category_initial=%lu category_tokens=%lu%n",
        setters, &initial, apps, &categoryInitial, tokens, &consumed) == 5
        && text[consumed] == 0 && initial == 0 && categoryInitial == 0;
#else
    *tokens = 0;
    return text && sscanf(text, "Selection snapshot setters=%lu initial=%lu apps=%lu%n",
                          setters, &initial, apps, &consumed) == 3
           && text[consumed] == 0 && initial == 0;
#endif
}
uint8_t observe_picker_category(XCTestCase *test, XCUIApplication *app) {
    keepImage(test, app, @"category-entry");
    XCUIElement *badge = app.buttons[@"Rust selection aggregate snapshot"];
    if (![badge waitForExistenceWithTimeout:25]) return 0;
    unsigned long before = 0, apps = 0, beforeTokens = 0;
    if (!snapshot(badge, &before, &apps, &beforeTokens)) return 0;
    printf("XCTest consent-selection: category_observation_started=true; applications_count=%lu\n", apps);
    fflush(stdout);
#if CATEGORY_TOKEN_COUNTS
    printf("XCTest consent-selection: category_tokens_before=%lu\n", beforeTokens);
    fflush(stdout);
    if (beforeTokens != 0) return 0;
#endif
    XCUIElementQuery *collections = app.collectionViews;
    XCUIElement *collection = nil;
    BOOL hittable = NO, social = NO;
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:12];
    do {
        if (collections.count == 1) {
            collection = collections.firstMatch;
            hittable = collection.hittable;
            social = collection.staticTexts[@"Social"].exists;
            if (hittable && social) break;
        }
        [NSThread sleepForTimeInterval:0.2];
    } while (deadline.timeIntervalSinceNow > 0);
    keepImage(test, app, @"category-surface-before-verdict");
    printf("XCTest consent-selection: category_surface_collection_count=%lu; hittable=%d; social_exists=%d\n",
           (unsigned long)collections.count, hittable, social);
    fflush(stdout);
    if (collections.count != 1 || !hittable || !social) return 0;
    XCUIElementQuery *rows = [collection.cells containingPredicate:
        [NSPredicate predicateWithFormat:@"label == %@", @"Social"]];
    printf("XCTest consent-selection: host_collection_count=%lu; category_cell_matches=%lu\n",
           (unsigned long)collections.count, (unsigned long)rows.count);
    fflush(stdout);
    keepImage(test, app, @"category-before");
    if (rows.count != 1 || !rows.firstMatch.hittable) return 0;
    XCUIElement *row = rows.firstMatch;
    if (CGRectIsEmpty(row.frame)) return 0;
    [[row coordinateWithNormalizedOffset:CGVectorMake(0.08, 0.5)] tap];
    unsigned long after = before, afterApps = apps, afterTokens = beforeTokens;
    bool verified = false;
    for (int i = 0; i < 25; i++) {
        if (snapshot(badge, &after, &afterApps, &afterTokens) && after > before) {
#if CATEGORY_TOKEN_COUNTS
            verified = afterTokens > 0;
#else
            verified = true;
#endif
            if (verified) break;
        }
        [NSThread sleepForTimeInterval:0.2];
    }
    keepImage(test, app, @"category-after");
#if CATEGORY_TOKEN_COUNTS
    printf("XCTest consent-selection: category_control_tap_returned=true; setter_before=%lu; setter_after=%lu; applications_count=%lu; category_tokens_before=%lu; category_tokens_after=%lu; category_token_count_verified=%s; individual_app_tap=false\n",
           before, after, afterApps, beforeTokens, afterTokens, verified ? "true" : "false");
#else
    printf("XCTest consent-selection: category_control_tap_returned=true; setter_before=%lu; setter_after=%lu; applications_count=%lu; category_token_count_verified=false; individual_app_tap=false\n",
           before, after, afterApps);
#endif
    fflush(stdout);
#if CAPTURED_SELECTION_TEARDOWN
    if (verified) {
        keepImage(test, app, @"owned-selection-before-close");
#if CONTEXT_ARC_TRACE
        NSString *description = app.debugDescription;
        NSRegularExpression *pattern = [NSRegularExpression regularExpressionWithPattern:@"pid: ([0-9]+)" options:0 error:nil];
        NSTextCheckingResult *match = [pattern firstMatchInString:description options:0 range:NSMakeRange(0, description.length)];
        if (!match) return 0;
        NSString *pid = [description substringWithRange:[match rangeAtIndex:1]];
        printf("XCTest context-trace: target_pid_before_close=%s\n", pid.UTF8String);
        fflush(stdout);
#endif
        if (!badge.hittable) return 0;
        [badge tap];
        BOOL exited = [app waitForState:XCUIApplicationStateNotRunning timeout:25];
        printf("XCTest consent-selection: owned_teardown_requested=true; target_not_running=%s; teardown_file_acceptance_required=true\n",
               exited ? "true" : "false");
        fflush(stdout);
        if (!exited) return 0;
    }
#endif
    return verified ? 1 : 0;
}
