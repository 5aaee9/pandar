use std::sync::atomic::{AtomicU32, Ordering};

const STUDIO_START_SEQUENCE_ID: u32 = 20000;
const STUDIO_END_SEQUENCE_ID: u32 = 30000;
static STUDIO_SEQUENCE_ID: AtomicU32 = AtomicU32::new(STUDIO_START_SEQUENCE_ID);

pub(super) fn next_studio_sequence_id() -> String {
    next_studio_sequence_id_from(&STUDIO_SEQUENCE_ID)
}

pub(crate) fn next_studio_sequence_id_from(sequence: &AtomicU32) -> String {
    loop {
        let current = sequence.load(Ordering::Relaxed);
        let sequence_id = if (STUDIO_START_SEQUENCE_ID..STUDIO_END_SEQUENCE_ID).contains(&current) {
            current
        } else {
            STUDIO_START_SEQUENCE_ID
        };
        let next = if sequence_id + 1 >= STUDIO_END_SEQUENCE_ID {
            STUDIO_START_SEQUENCE_ID
        } else {
            sequence_id + 1
        };

        if sequence
            .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return sequence_id.to_string();
        }
    }
}
