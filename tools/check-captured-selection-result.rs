use std::{collections::BTreeMap, env, fs, process};

type Fields<'a> = BTreeMap<&'a str, &'a str>;
fn parse(text: &str) -> Result<Fields<'_>, String> {
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or("incomplete result record")?;
        if key.is_empty() || value.is_empty() || fields.insert(key, value).is_some() {
            return Err("empty or duplicate result field".into());
        }
    }
    Ok(fields)
}
fn exact(fields: &Fields<'_>, key: &str, expected: &str) -> Result<(), String> {
    if fields.get(key).copied() == Some(expected) {
        Ok(())
    } else {
        Err(format!("required result field rejected: {key}"))
    }
}
fn number(fields: &Fields<'_>, key: &str) -> Result<u64, String> {
    fields
        .get(key)
        .filter(|value| value.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("numeric result field missing/invalid: {key}"))
}
fn require(accepted: bool, message: &str) -> Result<(), String> {
    if accepted {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn validate(teardown: &str, prior_selection: &str) -> Result<(), String> {
    let output = parse(teardown)?;
    let prior = parse(prior_selection)?;
    exact(&output, "phase", "complete")?;
    exact(&output, "preflight_phase", "complete")?;
    exact(&output, "picker_preflight_phase", "complete")?;
    for key in [
        "weak_initial_controller_nil",
        "weak_hosting_controller_nil",
        "weak_window_nil",
        "weak_button_nil",
        "weak_hosting_view_nil",
        "preflight_context_identity_cleared",
        "preflight_state_pointer_cleared",
        "picker_preflight_context_identity_cleared",
        "picker_preflight_state_pointer_cleared",
        "captured_context_identity_cleared",
        "state_pointer_cleared",
        "selection_state_watchdog_stopped_and_joined",
        "window_pointer_cleared",
        "controller_pointer_cleared",
        "button_pointer_cleared",
        "named_ui_and_state_graph_release_verified",
        "full_framework_teardown_not_established",
        "state_cleanup_does_not_cover_framework_caches",
    ] {
        exact(&output, key, "true")?;
    }
    for key in [
        "individual_app_selection_verified",
        "monitoring_verified",
        "shielding_verified",
    ] {
        exact(&output, key, "false")?;
    }
    for key in [
        "captured_context_allocations",
        "captured_selection_state_drop_completed",
        "captured_context_destroy_entered",
        "captured_context_deallocation_completed",
        "captured_context_root_releases",
        "initial_controller_reference_releases",
        "button_temporary_retains",
        "button_temporary_releases",
        "hosting_reference_releases",
        "window_reference_releases",
        "delegate_owned_reference_releases",
        "bootstrap_watchdog_joins",
        "proof_owners_before_final_drop",
        "close_probe_rejected_set_callbacks",
        "close_probe_binding_copy_destroy_calls",
        "preflight_binding_initializer_calls",
        "preflight_binding_copy_calls",
        "preflight_public_getter_calls",
        "preflight_context_allocations",
        "preflight_state_drop_completed",
        "preflight_context_destroy_entered",
        "preflight_context_deallocation_completed",
        "preflight_root_releases",
        "picker_preflight_binding_initializer_calls",
        "picker_preflight_public_getter_calls",
        "picker_preflight_context_allocations",
        "picker_preflight_state_drop_completed",
        "picker_preflight_context_destroy_entered",
        "picker_preflight_context_deallocation_completed",
        "picker_preflight_root_releases",
        "picker_preflight_picker_constructor_calls",
        "picker_preflight_picker_copy_calls",
    ] {
        require(
            number(&output, key)? == 1,
            "owned reference/callback completion count rejected",
        )?;
    }
    require(
        number(&output, "captured_context_input_retains")? == 2,
        "closure input retain count rejected",
    )?;
    require(
        number(&output, "preflight_binding_destroy_calls")? == 2,
        "standalone Binding destroys missing",
    )?;
    require(
        number(&output, "preflight_input_retains")? == 2,
        "standalone closure input retains missing",
    )?;
    for key in [
        "picker_preflight_binding_copy_calls",
        "picker_preflight_binding_destroy_calls",
        "picker_preflight_input_retains",
        "picker_preflight_picker_destroy_calls",
        "picker_preflight_picker_allocations",
        "picker_preflight_picker_deallocations",
    ] {
        require(
            number(&output, key)? == 2,
            "standalone Picker ownership count rejected",
        )?;
    }
    require(
        number(&output, "captured_context_deferred_main_drops")? <= 1,
        "deferred drop count rejected",
    )?;
    require(
        number(&output, "captured_validated_context_callbacks")? >= 4,
        "validated callbacks missing",
    )?;
    require(
        number(&output, "close_probe_readonly_get_callbacks")? >= 2,
        "copied Binding reads missing",
    )?;
    for (allocate, free) in [
        (
            "mutable_rust_value_allocations",
            "mutable_rust_value_deallocations",
        ),
        (
            "picker_rust_value_allocations",
            "picker_rust_value_deallocations",
        ),
        (
            "preflight_mutable_allocations",
            "preflight_mutable_deallocations",
        ),
        (
            "picker_preflight_mutable_allocations",
            "picker_preflight_mutable_deallocations",
        ),
    ] {
        let allocations = number(&output, allocate)?;
        require(
            allocations > 0 && allocations == number(&output, free)?,
            "Rust value buffers not balanced",
        )?;
    }
    let selected = number(&prior, "selected_category_tokens_count")?;
    require(
        selected > 0 && number(&prior, "initial_category_tokens_count")? == 0,
        "real nonempty category selection missing",
    )?;
    require(
        number(&prior, "selection_snapshot_category_tokens_count")? == selected,
        "Selection snapshot differs from real selection",
    )?;
    exact(&prior, "selection_snapshot_lifetime_verified", "true")?;
    require(
        number(&output, "close_probe_initial_category_count")? == selected
            && number(&output, "close_probe_final_category_count")? == selected
            && number(&output, "close_probe_empty_input_count")? == 0,
        "closing setter changed the nonempty selection or had no distinct input",
    )?;
    Ok(())
}
fn main() {
    let args: Vec<_> = env::args().collect();
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if args.len() != 3 {
            return Err("expected teardown result and pre-close selection result".into());
        }
        validate(
            &fs::read_to_string(&args[1])?,
            &fs::read_to_string(&args[2])?,
        )?;
        println!("named_ui_and_state_graph_result_accepted=true");
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("Captured Selection result rejected: {error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (String, String) {
        // Synthetic parser fixture only. No native value, opaque token, UI
        // action or actual lifetime result is constructed by these host tests.
        let text = concat!(
            "phase=complete\n",
            "preflight_phase=complete\npreflight_binding_initializer_calls=1\npreflight_binding_copy_calls=1\npreflight_binding_destroy_calls=2\npreflight_public_getter_calls=1\n",
            "preflight_context_allocations=1\npreflight_state_drop_completed=1\npreflight_context_destroy_entered=1\npreflight_context_deallocation_completed=1\npreflight_input_retains=2\npreflight_root_releases=1\n",
            "preflight_mutable_allocations=4\npreflight_mutable_deallocations=4\npreflight_context_identity_cleared=true\npreflight_state_pointer_cleared=true\nweak_hosting_view_nil=true\n",
            "picker_preflight_phase=complete\npicker_preflight_binding_initializer_calls=1\npicker_preflight_binding_copy_calls=2\npicker_preflight_binding_destroy_calls=2\npicker_preflight_public_getter_calls=1\n",
            "picker_preflight_context_allocations=1\npicker_preflight_state_drop_completed=1\npicker_preflight_context_destroy_entered=1\npicker_preflight_context_deallocation_completed=1\npicker_preflight_input_retains=2\npicker_preflight_root_releases=1\n",
            "picker_preflight_mutable_allocations=5\npicker_preflight_mutable_deallocations=5\npicker_preflight_context_identity_cleared=true\npicker_preflight_state_pointer_cleared=true\n",
            "picker_preflight_picker_constructor_calls=1\npicker_preflight_picker_copy_calls=1\npicker_preflight_picker_destroy_calls=2\npicker_preflight_picker_allocations=2\npicker_preflight_picker_deallocations=2\n",
            "weak_initial_controller_nil=true\nweak_hosting_controller_nil=true\nweak_window_nil=true\nweak_button_nil=true\n",
            "captured_context_identity_cleared=true\nstate_pointer_cleared=true\n",
            "selection_state_watchdog_stopped_and_joined=true\nwindow_pointer_cleared=true\ncontroller_pointer_cleared=true\nbutton_pointer_cleared=true\n",
            "named_ui_and_state_graph_release_verified=true\nfull_framework_teardown_not_established=true\nstate_cleanup_does_not_cover_framework_caches=true\n",
            "individual_app_selection_verified=false\nmonitoring_verified=false\nshielding_verified=false\n",
            "captured_context_allocations=1\ncaptured_selection_state_drop_completed=1\ncaptured_context_destroy_entered=1\ncaptured_context_deallocation_completed=1\n",
            "captured_context_root_releases=1\ninitial_controller_reference_releases=1\nbutton_temporary_retains=1\nbutton_temporary_releases=1\n",
            "hosting_reference_releases=1\nwindow_reference_releases=1\ndelegate_owned_reference_releases=1\nbootstrap_watchdog_joins=1\nproof_owners_before_final_drop=1\n",
            "close_probe_rejected_set_callbacks=1\nclose_probe_binding_copy_destroy_calls=1\ncaptured_context_input_retains=2\ncaptured_context_deferred_main_drops=0\n",
            "captured_validated_context_callbacks=8\nclose_probe_readonly_get_callbacks=2\nclose_probe_initial_category_count=1\nclose_probe_final_category_count=1\nclose_probe_empty_input_count=0\n",
            "mutable_rust_value_allocations=13\nmutable_rust_value_deallocations=13\npicker_rust_value_allocations=2\npicker_rust_value_deallocations=2\n"
        ).to_owned();
        (text, "initial_category_tokens_count=0\nselected_category_tokens_count=1\nselection_snapshot_category_tokens_count=1\nselection_snapshot_lifetime_verified=true\n".into())
    }
    #[test]
    fn accepts_complete_main_or_deferred_finalization() {
        let (result, prior) = fixture();
        validate(&result, &prior).unwrap();
        validate(
            &result.replace(
                "captured_context_deferred_main_drops=0",
                "captured_context_deferred_main_drops=1",
            ),
            &prior,
        )
        .unwrap();
    }
    #[test]
    fn rejects_each_missing_owner_and_semantic_condition() {
        let (result, prior) = fixture();
        for (from, to) in [
            ("phase=complete", "phase=waiting"),
            ("weak_window_nil=true", "weak_window_nil=false"),
            (
                "weak_hosting_controller_nil=true",
                "weak_hosting_controller_nil=unobserved",
            ),
            (
                "captured_selection_state_drop_completed=1",
                "captured_selection_state_drop_completed=0",
            ),
            (
                "captured_context_deallocation_completed=1",
                "captured_context_deallocation_completed=0",
            ),
            (
                "mutable_rust_value_deallocations=13",
                "mutable_rust_value_deallocations=12",
            ),
            (
                "picker_rust_value_deallocations=2",
                "picker_rust_value_deallocations=1",
            ),
            (
                "close_probe_rejected_set_callbacks=1",
                "close_probe_rejected_set_callbacks=0",
            ),
            (
                "close_probe_binding_copy_destroy_calls=1",
                "close_probe_binding_copy_destroy_calls=0",
            ),
            (
                "close_probe_final_category_count=1",
                "close_probe_final_category_count=0",
            ),
            (
                "close_probe_empty_input_count=0",
                "close_probe_empty_input_count=1",
            ),
            ("bootstrap_watchdog_joins=1", "bootstrap_watchdog_joins=0"),
            (
                "preflight_context_deallocation_completed=1",
                "preflight_context_deallocation_completed=0",
            ),
            ("preflight_phase=complete", "preflight_phase=not_drained"),
            ("weak_hosting_view_nil=true", "weak_hosting_view_nil=false"),
        ] {
            assert!(result.contains(from));
            assert!(validate(&result.replace(from, to), &prior).is_err());
        }
    }
    #[test]
    fn rejects_empty_truncated_duplicate_and_missing_nonempty_selection() {
        let (result, prior) = fixture();
        assert!(validate("", &prior).is_err());
        assert!(validate(&(result.clone() + "partial"), &prior).is_err());
        assert!(validate(&(result.clone() + "phase=complete\n"), &prior).is_err());
        assert!(
            validate(
                &result,
                &prior.replace(
                    "selected_category_tokens_count=1",
                    "selected_category_tokens_count=0"
                )
            )
            .is_err()
        );
        assert!(
            validate(
                &result,
                &prior.replace(
                    "selection_snapshot_category_tokens_count=1",
                    "selection_snapshot_category_tokens_count=0"
                )
            )
            .is_err()
        );
    }
    #[test]
    fn rejects_each_missing_or_incomplete_picker_preflight_field() {
        let (result, prior) = fixture();
        let fields: Vec<_> = result
            .lines()
            .filter(|line| line.starts_with("picker_preflight_"))
            .collect();
        assert_eq!(fields.len(), 20);
        for field in fields {
            let (key, value) = field.split_once('=').unwrap();
            let missing = result
                .lines()
                .filter(|line| *line != field)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            assert!(validate(&missing, &prior).is_err(), "missing {key}");
            assert!(
                validate(&(result.clone() + field + "\n"), &prior).is_err(),
                "duplicate {key}"
            );
            let rejected = if value == "complete" {
                "not_drained"
            } else if value == "true" {
                "false"
            } else {
                "0"
            };
            let changed = result.replace(field, &format!("{key}={rejected}"));
            assert_ne!(changed, result);
            assert!(validate(&changed, &prior).is_err(), "incomplete {key}");
        }
    }
}
