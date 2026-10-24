use super::*;

fn repeated_axes(path: &InputPath) -> Vec<InputPath> {
    let mut prefix = vec![];
    let mut axes = vec![];
    for step in path {
        prefix.push(step.clone());
        if matches!(step, DomainPathStep::Each) {
            axes.push(prefix.clone());
        }
    }
    axes
}

fn shares_repeated_axis(paths: &[InputPath]) -> bool {
    for (index, left) in paths.iter().enumerate() {
        let left_axes = repeated_axes(left);
        for right in &paths[index + 1..] {
            let right_axes = repeated_axes(right);
            if left_axes.iter().any(|axis| right_axes.contains(axis)) {
                return true;
            }
        }
    }
    false
}

impl DomainTarget {
    pub(super) fn is_covered(
        &self,
        coordinate: &CaseDomainCoordinate,
        occupied: &BTreeSet<Vec<(CaseDomainCoordinateSector, CaseDomainCoordinateSlot)>>,
    ) -> bool {
        let wanted: Vec<_> = coordinate
            .entries
            .iter()
            .filter(|e| {
                if self.violated.is_empty() {
                    self.focus.contains(&e.path)
                } else {
                    self.violated.contains(&e.sector.constraint)
                }
            })
            .map(|e| (e.sector.clone(), e.slot.clone()))
            .collect();
        if wanted.is_empty() {
            return false;
        }
        if self.composition.is_some() {
            return occupied.iter().any(|c| {
                wanted.iter().all(|(sector, slot)| {
                    if matches!(sector.region, DomainRegion::Range { .. }) {
                        c.iter().any(|(other, _)| other == sector)
                    } else {
                        c.contains(&(sector.clone(), slot.clone()))
                    }
                })
            });
        }
        if self.violated.len() > 1 {
            return occupied.iter().any(|c| {
                let negative_ids: BTreeSet<_> = c
                    .iter()
                    .filter(|(sector, _)| sector.region.is_negative())
                    .map(|(sector, _)| sector.constraint)
                    .collect();
                negative_ids.len() == self.violated.len()
                    && wanted.iter().all(|(s, _)| {
                        c.iter().any(|(other, _)| {
                            s.constraint == other.constraint && other.region.is_negative()
                        })
                    })
            });
        }
        let repeated_combination =
            self.violated.is_empty() && self.focus.len() > 1 && shares_repeated_axis(&self.focus);
        occupied.iter().any(|c| {
            wanted.iter().all(|entry| c.contains(entry))
                && (!repeated_combination
                    || wanted.iter().all(|(sector, _)| {
                        c.iter()
                            .filter(|(other, _)| other.constraint == sector.constraint)
                            .count()
                            == 1
                    }))
        })
    }
}
