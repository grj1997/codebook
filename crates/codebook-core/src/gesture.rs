#[derive(Clone, Copy)]
pub struct RightClick {
    pub at_ms: u64,
    pub x: i32,
    pub y: i32,
    pub target_id: u64,
    pub editable: bool,
    pub injected: bool,
}
#[derive(Default)]
pub struct DoubleRightClick {
    first: Option<RightClick>,
    last_trigger: Option<u64>,
}
impl DoubleRightClick {
    pub fn click(
        &mut self,
        event: RightClick,
        interval: u64,
        width: i32,
        height: i32,
    ) -> Option<RightClick> {
        if event.injected {
            return None;
        }
        if self
            .last_trigger
            .is_some_and(|at| event.at_ms.saturating_sub(at) < interval)
        {
            return None;
        }
        if let Some(first) = self.first.take() {
            if event.at_ms.saturating_sub(first.at_ms) <= interval
                && first.target_id == event.target_id
                && first.x.abs_diff(event.x) <= width.max(1) as u32
                && first.y.abs_diff(event.y) <= height.max(1) as u32
            {
                self.last_trigger = Some(event.at_ms);
                return Some(first);
            }
        }
        if event.editable {
            self.first = Some(event);
        }
        None
    }
}
