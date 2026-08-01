use pandar_core::{BambuNozzleDevice, BambuNozzleHolder, BambuNozzleSystem};

use crate::machine::mqtt::snapshot::NozzleSystemPatch;

#[derive(Default)]
pub(super) struct NozzleSystemReducer {
    nozzle: Option<BambuNozzleDevice>,
    holder: Option<BambuNozzleHolder>,
}

impl NozzleSystemReducer {
    pub(super) fn update(&mut self, patch: NozzleSystemPatch) -> Option<BambuNozzleSystem> {
        if let Some(nozzle) = patch.nozzle {
            self.nozzle = Some(nozzle);
        }
        if let Some(holder) = patch.holder {
            self.holder = Some(holder);
        }
        self.nozzle.clone().map(|nozzle| BambuNozzleSystem {
            nozzle,
            holder: self.holder.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use pandar_core::{BambuNozzleInfo, StudioFiniteF64};

    use super::*;

    fn nozzle(id: i32) -> BambuNozzleDevice {
        BambuNozzleDevice {
            exist: Some(1),
            state: Some(0),
            src_id: None,
            tar_id: None,
            info: vec![BambuNozzleInfo {
                id,
                diameter: StudioFiniteF64::try_from(0.4).unwrap(),
                nozzle_type: "XS01".to_owned(),
                stat: Some(0),
                fila_id: None,
                wear: None,
                p_t: None,
                color_m: None,
            }],
        }
    }

    fn holder(pos: i32) -> BambuNozzleHolder {
        BambuNozzleHolder {
            stat: Some(0),
            pos: Some(pos),
            info: Some(0),
        }
    }

    #[test]
    fn holder_delta_before_nozzle_is_retained() {
        let mut reducer = NozzleSystemReducer::default();
        assert!(
            reducer
                .update(NozzleSystemPatch {
                    nozzle: None,
                    holder: Some(holder(2)),
                })
                .is_none()
        );

        let system = reducer
            .update(NozzleSystemPatch {
                nozzle: Some(nozzle(16)),
                holder: None,
            })
            .unwrap();
        assert_eq!(system.nozzle.info[0].id, 16);
        assert_eq!(system.holder.unwrap().pos, Some(2));
    }

    #[test]
    fn nozzle_and_holder_deltas_do_not_erase_each_other() {
        let mut reducer = NozzleSystemReducer::default();
        reducer.update(NozzleSystemPatch {
            nozzle: Some(nozzle(16)),
            holder: Some(holder(1)),
        });
        let after_nozzle = reducer
            .update(NozzleSystemPatch {
                nozzle: Some(nozzle(17)),
                holder: None,
            })
            .unwrap();
        assert_eq!(after_nozzle.nozzle.info[0].id, 17);
        assert_eq!(after_nozzle.holder.unwrap().pos, Some(1));

        let after_holder = reducer
            .update(NozzleSystemPatch {
                nozzle: None,
                holder: Some(holder(3)),
            })
            .unwrap();
        assert_eq!(after_holder.nozzle.info[0].id, 17);
        assert_eq!(after_holder.holder.unwrap().pos, Some(3));
    }
}
