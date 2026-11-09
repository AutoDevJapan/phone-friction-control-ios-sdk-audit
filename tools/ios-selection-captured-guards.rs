// SDK18.5 common constructor guard reused from the executed captured-Bool
// baseline. It does not transfer the Bool value layout or copy-VWT guards to
// Selection. Existing Selection56/Binding72 VWT checks apply separately.
fn verify_selection_capture_constructor(entry: *const c_void, page: usize) {
    category_image(entry, b"/SwiftUICore");
    let helper = branch(entry, 16, false, page);
    category_image(helper, b"/SwiftUICore");
    category_words(
        helper,
        &[
            (0, 0xffffffffu32, 0xa9ba6ffcu32),
            (4, 0xffffffffu32, 0xa90167fau32),
            (8, 0xffffffffu32, 0xa9025ff8u32),
            (12, 0xffffffffu32, 0xa90357f6u32),
            (16, 0xffffffffu32, 0xa9044ff4u32),
            (20, 0xffffffffu32, 0xa9057bfdu32),
            (24, 0xffffffffu32, 0x910143fdu32),
            (28, 0xffffffffu32, 0xd100c3ffu32),
            (32, 0xffffffffu32, 0xaa0603f4u32),
            (36, 0xffffffffu32, 0xaa0403f8u32),
            (40, 0xffffffffu32, 0xaa0303f6u32),
            (44, 0xffffffffu32, 0xaa0203f9u32),
            (48, 0xffffffffu32, 0xaa0103f7u32),
            (52, 0xffffffffu32, 0xaa0003fau32),
            (56, 0xffffffffu32, 0xf81883a8u32),
            (60, 0xffffffffu32, 0xf85f8088u32),
            (64, 0xffffffffu32, 0xf9402108u32),
            (68, 0xffffffffu32, 0x910003e9u32),
            (72, 0xffffffffu32, 0x91003d08u32),
            (76, 0xffffffffu32, 0x927ced08u32),
            (80, 0xffffffffu32, 0xcb080135u32),
            (84, 0xffffffffu32, 0x910002bfu32),
            (88, 0xffffffffu32, 0xaa0503e0u32),
            (92, 0xffffffffu32, 0x52800501u32),
            (96, 0xffffffffu32, 0x528000e2u32),
            (100, 0xfc000000u32, 0x94000000u32),
            (104, 0xffffffffu32, 0xaa0003e3u32),
            (108, 0xffffffffu32, 0xa9016418u32),
            (112, 0xffffffffu32, 0xf9001016u32),
            (116, 0xffffffffu32, 0xaa1a03e0u32),
            (120, 0xffffffffu32, 0xaa1703e1u32),
            (124, 0xffffffffu32, 0xaa1403e2u32),
            (128, 0xffffffffu32, 0xaa1803e4u32),
            (132, 0xfc000000u32, 0x94000000u32),
            (136, 0xffffffffu32, 0xaa0003f9u32),
            (140, 0xffffffffu32, 0xaa0103fbu32),
            (144, 0xffffffffu32, 0xaa0203fcu32),
            (148, 0xffffffffu32, 0xaa0303f3u32),
            (152, 0xffffffffu32, 0xaa1703e0u32),
            (156, 0xfc000000u32, 0x94000000u32),
            (160, 0xffffffffu32, 0xaa1603e0u32),
            (164, 0xfc000000u32, 0x94000000u32),
            (168, 0xffffffffu32, 0xaa1503e8u32),
            (172, 0xffffffffu32, 0xaa1703f4u32),
            (176, 0xffffffffu32, 0xd63f0340u32),
            (180, 0xffffffffu32, 0x52801fe0u32),
            (184, 0xffffffffu32, 0xaa1803e1u32),
            (188, 0xfc000000u32, 0x94000000u32),
            (192, 0xffffffffu32, 0xaa0003f4u32),
            (196, 0x9f00001fu32, 0x90000000u32),
            (200, 0xffffffffu32, 0x911b0000u32),
            (204, 0xffffffffu32, 0xaa1403e1u32),
            (208, 0xfc000000u32, 0x94000000u32),
            (212, 0xffffffffu32, 0xaa0003e2u32),
            (216, 0xffffffffu32, 0xd2800000u32),
            (220, 0xffffffffu32, 0xaa1403e1u32),
            (224, 0xfc000000u32, 0x94000000u32),
            (228, 0xffffffffu32, 0xaa0003f4u32),
            (232, 0xffffffffu32, 0xa9396fb9u32),
            (236, 0xffffffffu32, 0xa93a4fbcu32),
            (240, 0xffffffffu32, 0xd101c3a0u32),
            (244, 0xfc000000u32, 0x94000000u32),
            (248, 0xffffffffu32, 0xaa0003f3u32),
            (252, 0xffffffffu32, 0xaa1703e0u32),
            (256, 0xfc000000u32, 0x94000000u32),
            (260, 0xffffffffu32, 0xaa1603e0u32),
            (264, 0xfc000000u32, 0x94000000u32),
            (268, 0xffffffffu32, 0xf9400268u32),
            (272, 0xffffffffu32, 0xf85883a9u32),
            (276, 0xffffffffu32, 0xa9004d3fu32),
            (280, 0xffffffffu32, 0xaa0903f4u32),
            (284, 0xffffffffu32, 0xf9402d13u32),
            (288, 0xffffffffu32, 0xd2800000u32),
            (292, 0xffffffffu32, 0xaa1303e1u32),
            (296, 0xfc000000u32, 0x94000000u32),
            (300, 0xffffffffu32, 0xb9802008u32),
            (304, 0xffffffffu32, 0xf85f8269u32),
            (308, 0xffffffffu32, 0xf9401129u32),
            (312, 0xffffffffu32, 0x8b080280u32),
            (316, 0xffffffffu32, 0xaa1503e1u32),
            (320, 0xffffffffu32, 0xaa1303e2u32),
            (324, 0xffffffffu32, 0xd63f0120u32),
            (328, 0xffffffffu32, 0xd10143bfu32),
            (332, 0xffffffffu32, 0xa9457bfdu32),
            (336, 0xffffffffu32, 0xa9444ff4u32),
            (340, 0xffffffffu32, 0xa94357f6u32),
            (344, 0xffffffffu32, 0xa9425ff8u32),
            (348, 0xffffffffu32, 0xa94167fau32),
            (352, 0xffffffffu32, 0xa8c66ffcu32),
            (356, 0xffffffffu32, 0xd65f03c0u32),
        ],
        page,
    );
    for (offset, name) in [
        (0x9c, "swift_retain"),
        (0xa4, "swift_retain"),
        (0x100, "swift_release"),
        (0x108, "swift_release"),
    ] {
        category_runtime_stub(
            branch(helper, offset, true, page),
            b"/SwiftUICore",
            name,
            page,
        );
    }
}

fn verify_selection_capture_witness_images(value: &Witnesses) {
    for entry in [
        value.destroy as *const c_void,
        value.initialize as *const c_void,
        value.assign as *const c_void,
    ] {
        let mut info = DlInfo::default();
        assert!(unsafe { dladdr(entry, &mut info) } != 0);
        assert!(!info.base.is_null() && !info.filename.is_null());
        let image = unsafe { std::ffi::CStr::from_ptr(info.filename) }.to_bytes();
        assert!(
            [
                b"/SwiftUICore".as_slice(),
                b"/FamilyControls".as_slice(),
                b"/libswiftCore.dylib".as_slice()
            ]
            .iter()
            .any(|suffix| image.ends_with(suffix))
        );
    }
}
