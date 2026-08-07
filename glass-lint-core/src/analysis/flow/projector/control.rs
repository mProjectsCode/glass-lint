//! Control-flow state transitions for the object-flow projector.
//!
//! A control boundary carries a bounded list of checkpoint-backed alternatives.
//! The alternatives stay correlated: a later transfer is replayed from each
//! checkpoint, rather than being applied to independently joined alias and
//! lifecycle sets.

use crate::analysis::{
    facts::{ControlRegionId, FactId},
    flow::projector::{AbruptExit, ControlFrame, ControlKind, ObjectFlowProjector},
};

impl ObjectFlowProjector<'_, '_, '_> {
    pub(super) fn transfer_control(
        &mut self,
        kind: ControlKind,
        region: ControlRegionId,
        fact: FactId,
    ) {
        match kind {
            ControlKind::BranchStart
            | ControlKind::BranchThen
            | ControlKind::BranchElse
            | ControlKind::BranchEnd => self.transfer_branch(kind, region),
            ControlKind::LoopStart { .. } | ControlKind::LoopUpdate | ControlKind::LoopEnd => {
                self.transfer_loop(kind, region, fact);
            }
            ControlKind::SwitchStart | ControlKind::SwitchCase { .. } | ControlKind::SwitchEnd => {
                self.transfer_switch(kind, region);
            }
            ControlKind::TryStart
            | ControlKind::CatchStart
            | ControlKind::FinallyStart
            | ControlKind::TryEnd => self.transfer_try(kind, region),
            ControlKind::Break | ControlKind::Continue | ControlKind::Return => {
                self.transfer_abrupt(kind);
            }
        }
    }

    fn transfer_branch(&mut self, kind: ControlKind, region: ControlRegionId) {
        match kind {
            ControlKind::BranchStart => {
                self.control.push(ControlFrame::Branch {
                    region,
                    base: self.frontier.snapshot_paths(),
                    then_exit: None,
                });
            }
            ControlKind::BranchThen => {
                if let Some(ControlFrame::Branch { base, .. }) =
                    self.control.last_matching_mut(region)
                {
                    *base = self.frontier.snapshot_paths();
                }
            }
            ControlKind::BranchElse => {
                let base = if let Some(ControlFrame::Branch {
                    base, then_exit, ..
                }) = self.control.last_matching_mut(region)
                {
                    *then_exit = Some(self.frontier.snapshot_paths());
                    Some(base.clone())
                } else {
                    None
                };
                if let Some(base) = base {
                    self.frontier.replace_paths(base);
                }
            }
            ControlKind::BranchEnd => {
                let Some(ControlFrame::Branch {
                    base, then_exit, ..
                }) = self.control.pop_region(region)
                else {
                    return;
                };
                let mut paths = then_exit.unwrap_or(base);
                paths.extend(self.frontier.take_paths());
                self.join_paths(paths);
            }
            _ => unreachable!(),
        }
    }

    fn transfer_loop(&mut self, kind: ControlKind, region: ControlRegionId, fact: FactId) {
        match kind {
            ControlKind::LoopStart { guaranteed } => {
                self.control.push(ControlFrame::Loop {
                    region,
                    body_start: FactId::new(fact.raw().saturating_add(1)),
                    baseline: self.frontier.snapshot_paths(),
                    guaranteed,
                    breaks: Vec::new(),
                    continues: Vec::new(),
                });
            }
            ControlKind::LoopUpdate => {
                let continues = self.control.take_loop_continues();
                self.frontier.append_paths(continues);
                let paths = self.frontier.take_paths();
                self.join_paths(paths);
            }
            ControlKind::LoopEnd => {
                let Some(ControlFrame::Loop {
                    body_start,
                    baseline,
                    guaranteed,
                    breaks,
                    continues,
                    ..
                }) = self.control.loop_frame(region)
                else {
                    return;
                };
                self.finish_loop(body_start, fact, guaranteed, baseline, breaks, continues);
            }
            _ => unreachable!(),
        }
    }

    fn transfer_switch(&mut self, kind: ControlKind, region: ControlRegionId) {
        match kind {
            ControlKind::SwitchStart => {
                self.control.push(ControlFrame::Switch {
                    region,
                    baseline: self.frontier.snapshot_paths(),
                    breaks: Vec::new(),
                    has_default: false,
                });
            }
            ControlKind::SwitchCase { is_default } => {
                let (baseline, current) = match self.control.last_matching_mut(region) {
                    Some(ControlFrame::Switch {
                        baseline,
                        has_default,
                        ..
                    }) => {
                        *has_default |= is_default;
                        (baseline.clone(), self.frontier.take_paths())
                    }
                    _ => return,
                };
                let mut paths = baseline;
                paths.extend(current);
                self.join_paths(paths);
            }
            ControlKind::SwitchEnd => {
                let Some(ControlFrame::Switch {
                    baseline,
                    breaks,
                    has_default,
                    ..
                }) = self.control.pop_region(region)
                else {
                    return;
                };
                let mut paths = self.frontier.take_paths();
                paths.extend(breaks);
                if !has_default {
                    paths.extend(baseline);
                }
                self.join_paths(paths);
            }
            _ => unreachable!(),
        }
    }

    fn transfer_try(&mut self, kind: ControlKind, region: ControlRegionId) {
        match kind {
            ControlKind::TryStart => {
                self.control.push(ControlFrame::Try {
                    region,
                    baseline: self.frontier.snapshot_paths(),
                    try_exit: None,
                    catch_exit: None,
                    normal_exit: None,
                    abrupt_exits: Vec::new(),
                    has_finally: false,
                    normal_count: 0,
                });
            }
            ControlKind::CatchStart => {
                let baseline = match self.control.last_matching_mut(region) {
                    Some(ControlFrame::Try {
                        baseline, try_exit, ..
                    }) => {
                        *try_exit = Some(self.frontier.take_paths());
                        baseline.clone()
                    }
                    _ => return,
                };
                self.frontier.replace_paths(baseline);
            }
            ControlKind::FinallyStart => self.start_finally(region),
            ControlKind::TryEnd => self.end_try(region),
            _ => unreachable!(),
        }
    }

    fn start_finally(&mut self, region: ControlRegionId) {
        let current = self.frontier.take_paths();
        let incoming = if let Some(ControlFrame::Try {
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            normal_count,
            ..
        }) = self.control.last_matching_mut(region)
        {
            *catch_exit = Some(current.clone());
            *has_finally = true;
            let mut normal = try_exit.clone().unwrap_or_default();
            normal.extend(current.iter().copied());
            *normal_count = normal.len();
            *normal_exit = Some(normal.clone());
            let mut incoming = normal;
            incoming.extend(abrupt_exits.iter().map(|(_, environment)| *environment));
            incoming
        } else {
            current
        };
        self.join_paths(incoming);
    }

    fn end_try(&mut self, region: ControlRegionId) {
        let Some(ControlFrame::Try {
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            normal_count,
            ..
        }) = self.control.pop_region(region)
        else {
            return;
        };
        if has_finally {
            let after = self.frontier.take_paths();
            let normal_len = normal_count.min(after.len());
            let normal = after[..normal_len].to_vec();
            for (abrupt_index, (kind, _)) in (normal_len..).zip(abrupt_exits) {
                let Some(environment) = after.get(abrupt_index).copied() else {
                    break;
                };
                self.control.route_abrupt(kind, environment);
            }
            self.frontier.replace_paths(normal);
        } else {
            let mut paths = try_exit.unwrap_or_default();
            paths.extend(catch_exit.unwrap_or_else(|| normal_exit.unwrap_or_default()));
            paths.extend(self.frontier.take_paths());
            self.join_paths(paths);
        }
    }

    fn transfer_abrupt(&mut self, kind: ControlKind) {
        let abrupt = match kind {
            ControlKind::Break => AbruptExit::Break,
            ControlKind::Continue => AbruptExit::Continue,
            ControlKind::Return => AbruptExit::Return,
            _ => unreachable!(),
        };
        let current = self.frontier.take_paths();
        for environment in &current {
            self.control.record_abrupt_exit(abrupt, environment);
        }
        for environment in current {
            self.control.route_abrupt(abrupt, environment);
        }
    }
}
