#[path = "firmware_status/generation.rs"]
mod generation;
#[path = "firmware_status/render.rs"]
mod render;
#[path = "firmware_status/support.rs"]
mod support;
#[path = "firmware_status/validation.rs"]
mod validation;

macro_rules! test_case {
    ($name:ident, $module:ident) => {
        #[test]
        fn $name() {
            $module::$name();
        }
    };
}

test_case!(
    firmware_status_renders_exact_current_modules_upgrade_state_and_cfg,
    render
);
test_case!(
    firmware_catalog_has_exact_envelope_and_filters_only_empty_urls,
    render
);
test_case!(
    firmware_status_emits_exact_reset_immediately_and_past_three_seconds,
    render
);
test_case!(
    firmware_status_fresh_current_state_cancels_reset_repetition,
    render
);
test_case!(
    newer_firmware_identity_marker_resets_and_rejects_late_old_generation,
    generation
);
test_case!(
    newer_firmware_identity_partial_state_follows_one_exact_reset,
    generation
);
test_case!(
    fresh_current_after_invalidation_waits_for_one_exact_reset,
    generation
);
test_case!(
    delayed_lower_revisions_do_not_overwrite_newer_current_state,
    generation
);
test_case!(
    delayed_unseen_session_does_not_overwrite_newer_observation,
    generation
);
test_case!(
    delayed_same_identity_observations_cannot_undo_invalidation_but_newer_equal_can_recover,
    generation
);
test_case!(
    malformed_higher_sequence_does_not_block_lower_valid_typed_batch,
    validation
);
test_case!(never_populated_marker_does_not_fabricate_reset, validation);
test_case!(
    firmware_status_rejects_malformed_typed_batch_member_without_mutation,
    validation
);
