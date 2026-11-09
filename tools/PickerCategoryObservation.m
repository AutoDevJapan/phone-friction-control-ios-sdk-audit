#import <XCTest/XCTest.h>
#include <stdint.h>
#include <stdio.h>

static void keepImage(XCTestCase *test, XCUIApplication *app, NSString *name) {
    XCTAttachment *a = [XCTAttachment attachmentWithScreenshot:app.screenshot];
    a.name = name;
    a.lifetime = XCTAttachmentLifetimeKeepAlways;
    [test addAttachment:a];
}
static bool snapshot(XCUIElement *badge, unsigned long *setters, unsigned long *apps) {
    unsigned long initial = 0;
    int consumed = 0;
    const char *text = badge.label.UTF8String;
    return text && sscanf(text, "Selection snapshot setters=%lu initial=%lu apps=%lu%n",
                          setters, &initial, apps, &consumed) == 3
           && text[consumed] == 0 && initial == 0;
}
uint8_t observe_picker_category(XCTestCase *test, XCUIApplication *app) {
    XCUIElement *badge = app.buttons[@"Rust selection aggregate snapshot"];
    if (![badge waitForExistenceWithTimeout:25]) return 0;
    unsigned long before = 0, apps = 0;
    if (!snapshot(badge, &before, &apps)) return 0;
    printf("XCTest consent-selection: category_observation_started=true; applications_count=%lu\n", apps); fflush(stdout);
    XCUIElementQuery *collections = app.collectionViews;
    if (collections.count != 1) return 0;
    XCUIElement *collection = collections.firstMatch;
    if (!collection.hittable || !collection.staticTexts[@"Social"].exists) return 0;
    XCUIElementQuery *rows = [collection.cells containingPredicate:[NSPredicate predicateWithFormat:@"label == %@", @"Social"]];
    printf("XCTest consent-selection: host_collection_count=%lu; category_cell_matches=%lu\n", (unsigned long)collections.count, (unsigned long)rows.count); fflush(stdout);
    keepImage(test, app, @"category-before");
    if (rows.count != 1 || !rows.firstMatch.hittable) return 0;
    XCUIElement *row = rows.firstMatch;
    if (CGRectIsEmpty(row.frame)) return 0;
    [[row coordinateWithNormalizedOffset:CGVectorMake(0.08, 0.5)] tap];
    unsigned long after = before, afterApps = apps;
    for (int i = 0; i < 25; i++) {
        if (snapshot(badge, &after, &afterApps) && after > before) break;
        [NSThread sleepForTimeInterval:0.2];
    }
    keepImage(test, app, @"category-after");
    printf("XCTest consent-selection: category_control_tap_returned=true; setter_before=%lu; setter_after=%lu; applications_count=%lu; category_token_count_verified=false; individual_app_tap=false\n",
          before, after, afterApps); fflush(stdout);
    return after > before ? 1 : 0;
}
