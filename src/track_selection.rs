//! Entry-ID based selection and ordering; duplicate URLs remain distinct tracks.
use std::collections::BTreeSet;

#[derive(Default)]
pub struct TrackSelection {
    pub ids: BTreeSet<i64>,
    anchor: Option<i64>,
}

impl TrackSelection {
    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }

    pub fn retain(&mut self, order: &[i64]) {
        self.ids.retain(|id| order.contains(id));
        if self.anchor.is_some_and(|id| !order.contains(&id)) {
            self.anchor = None;
        }
    }

    /// Return whether an ordinary click should collapse the selection on release.
    pub fn press(&mut self, order: &[i64], index: usize, ctrl: bool, shift: bool) -> bool {
        let id = order[index];
        if shift {
            let anchor = self
                .anchor
                .and_then(|id| order.iter().position(|entry| *entry == id))
                .unwrap_or(index);
            if !ctrl {
                self.ids.clear();
            }
            self.ids
                .extend(order[index.min(anchor)..=index.max(anchor)].iter().copied());
            self.anchor.get_or_insert(id);
            false
        } else if ctrl {
            if !self.ids.remove(&id) {
                self.ids.insert(id);
            }
            self.anchor = Some(id);
            false
        } else {
            let collapse = self.ids.contains(&id);
            if !collapse {
                self.only(id);
            }
            collapse
        }
    }

    pub fn only(&mut self, id: i64) {
        self.ids.clear();
        self.ids.insert(id);
        self.anchor = Some(id);
    }
}

/// Move selected entries as one block, preserving playlist order, before the boundary.
pub fn moved_order(
    order: &[i64],
    selected: &[i64],
    before: Option<i64>,
) -> anyhow::Result<Vec<i64>> {
    anyhow::ensure!(!selected.is_empty(), "No tracks selected");
    let selected: BTreeSet<_> = selected.iter().copied().collect();
    anyhow::ensure!(
        selected.iter().all(|id| order.contains(id)),
        "Selected track is no longer in this playlist"
    );
    let boundary = match before {
        Some(id) => order
            .iter()
            .position(|entry| *entry == id)
            .ok_or_else(|| anyhow::anyhow!("Drop target is no longer in this playlist"))?,
        None => order.len(),
    };
    let target = order[..boundary]
        .iter()
        .filter(|id| !selected.contains(id))
        .count();
    let moving = order.iter().filter(|id| selected.contains(id)).copied();
    let mut remaining: Vec<_> = order
        .iter()
        .filter(|id| !selected.contains(id))
        .copied()
        .collect();
    remaining.splice(target..target, moving);
    Ok(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctrl_shift_and_deferred_plain_click() {
        let mut s = TrackSelection::default();
        let order = [10, 20, 30, 40, 50];
        assert!(!s.press(&order, 1, false, false));
        s.press(&order, 3, true, false);
        assert_eq!(s.ids, BTreeSet::from([20, 40]));
        assert!(s.press(&order, 1, false, false));
        assert_eq!(s.ids.len(), 2); // A drag keeps the group.
        s.only(20); // A click release collapses it.
        s.press(&order, 4, false, true);
        assert_eq!(s.ids, BTreeSet::from([20, 30, 40, 50]));
        s.press(&order, 2, true, false);
        assert!(!s.ids.contains(&30));
        s.press(&order, 0, true, true);
        assert_eq!(s.ids, BTreeSet::from(order));
        s.retain(&[10, 50]);
        assert_eq!(s.ids, BTreeSet::from([10, 50]));
    }

    #[test]
    fn block_moves_preserve_order_and_normalize_selected_boundaries() {
        let order = [1, 2, 3, 4, 5, 6];
        assert_eq!(
            moved_order(&order, &[5, 2], Some(4)).unwrap(),
            [1, 3, 2, 5, 4, 6]
        );
        assert_eq!(
            moved_order(&order, &[2, 5], None).unwrap(),
            [1, 3, 4, 6, 2, 5]
        );
        assert_eq!(
            moved_order(&order, &[2, 5], Some(1)).unwrap(),
            [2, 5, 1, 3, 4, 6]
        );
        assert_eq!(moved_order(&order, &[2, 3], Some(3)).unwrap(), order);
        assert_eq!(moved_order(&order, &order, None).unwrap(), order);
        assert!(moved_order(&order, &[99], None).is_err());
        assert!(moved_order(&order, &[2], Some(99)).is_err());
    }
    #[test]
    fn sequential_remote_moves_match_atomic_plan_for_every_subset_and_boundary() {
        let original = [1, 2, 3, 4, 5, 6];
        for mask in 1..64 {
            let selected: Vec<_> = original
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| *id)
                .collect();
            for boundary in 0..=original.len() {
                let desired =
                    moved_order(&original, &selected, original.get(boundary).copied()).unwrap();
                let end = desired
                    .iter()
                    .rposition(|id| selected.contains(id))
                    .unwrap();
                let mut anchor = desired.get(end + 1).copied();
                let mut actual = original.to_vec();
                for id in selected.iter().rev() {
                    actual = moved_order(&actual, &[*id], anchor).unwrap();
                    anchor = Some(*id);
                }
                assert_eq!(actual, desired, "mask {mask}, boundary {boundary}");
            }
        }
    }
}
