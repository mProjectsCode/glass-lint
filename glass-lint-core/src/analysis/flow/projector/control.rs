//! Control-flow state transitions for the object-flow projector.
//!
//! A control boundary carries a bounded list of checkpoint-backed alternatives.
//! The alternatives stay correlated: a later transfer is replayed from each
//! checkpoint, rather than being applied to independently joined alias and
//! lifecycle sets.

use crate::analysis::{
    facts::{ControlRegionId, FactId},
    flow::projector::{
        AbruptExit, ControlFrame, ControlKind, FlowEnvironment, ObjectFlowProjector,
    },
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
                    base: self.paths.clone(),
                    then_exit: None,
                });
            }
            ControlKind::BranchThen => {
                if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    base.clone_from(&self.paths);
                }
            }
            ControlKind::BranchElse => {
                let base = if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *then_exit = Some(self.paths.clone());
                    Some(base.clone())
                } else {
                    None
                };
                if let Some(base) = base {
                    self.paths = base;
                }
            }
            ControlKind::BranchEnd => {
                let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut paths = then_exit.unwrap_or(base);
                paths.append(&mut self.paths);
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
                    body_start: FactId(fact.0.saturating_add(1)),
                    baseline: self.paths.clone(),
                    guaranteed,
                    breaks: Vec::new(),
                    continues: Vec::new(),
                });
            }
            ControlKind::LoopUpdate => {
                let continues = match self.control.last_mut() {
                    Some(ControlFrame::Loop { continues, .. }) => std::mem::take(continues),
                    _ => Vec::new(),
                };
                self.paths.extend(continues);
                let paths = std::mem::take(&mut self.paths);
                self.join_paths(paths);
            }
            ControlKind::LoopEnd => {
                let Some(ControlFrame::Loop {
                    region: expected,
                    body_start,
                    baseline,
                    guaranteed,
                    breaks,
                    continues,
                }) = self.control.last().cloned()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
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
                    baseline: self.paths.clone(),
                    breaks: Vec::new(),
                    has_default: false,
                });
            }
            ControlKind::SwitchCase { is_default } => {
                let (baseline, current) = match self.control.last_mut() {
                    Some(ControlFrame::Switch {
                        region: expected,
                        baseline,
                        has_default,
                        ..
                    }) if *expected == region => {
                        *has_default |= is_default;
                        (baseline.clone(), std::mem::take(&mut self.paths))
                    }
                    _ => return,
                };
                let mut paths = baseline;
                paths.extend(current);
                self.join_paths(paths);
            }
            ControlKind::SwitchEnd => {
                let Some(ControlFrame::Switch {
                    region: expected,
                    baseline,
                    breaks,
                    has_default,
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut paths = std::mem::take(&mut self.paths);
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
                    baseline: self.paths.clone(),
                    try_exit: None,
                    catch_exit: None,
                    normal_exit: None,
                    abrupt_exits: Vec::new(),
                    has_finally: false,
                    normal_count: 0,
                });
            }
            ControlKind::CatchStart => {
                let baseline = match self.control.last_mut() {
                    Some(ControlFrame::Try {
                        region: expected,
                        baseline,
                        try_exit,
                        ..
                    }) if *expected == region => {
                        *try_exit = Some(std::mem::take(&mut self.paths));
                        baseline.clone()
                    }
                    _ => return,
                };
                self.paths = baseline;
            }
            ControlKind::FinallyStart => self.start_finally(region),
            ControlKind::TryEnd => self.end_try(region),
            _ => unreachable!(),
        }
    }

    fn start_finally(&mut self, region: ControlRegionId) {
        let current = std::mem::take(&mut self.paths);
        let incoming = if let Some(ControlFrame::Try {
            region: expected,
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            normal_count,
            ..
        }) = self.control.last_mut()
            && *expected == region
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
            region: expected,
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            normal_count,
            ..
        }) = self.control.pop()
        else {
            return;
        };
        if expected != region {
            return;
        }
        if has_finally {
            let after = std::mem::take(&mut self.paths);
            let normal_len = normal_count.min(after.len());
            let normal = after[..normal_len].to_vec();
            for (abrupt_index, (kind, _)) in (normal_len..).zip(abrupt_exits) {
                let Some(environment) = after.get(abrupt_index).copied() else {
                    break;
                };
                self.route_finally_abrupt(kind, environment);
            }
            self.paths = normal;
        } else {
            let mut paths = try_exit.unwrap_or_default();
            paths.extend(catch_exit.unwrap_or_else(|| normal_exit.unwrap_or_default()));
            paths.append(&mut self.paths);
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
        let current = std::mem::take(&mut self.paths);
        for environment in &current {
            self.record_abrupt_exit(abrupt, environment);
        }
        match abrupt {
            AbruptExit::Break => {
                if let Some(frame) = self.control.iter_mut().rev().find(|frame| {
                    matches!(
                        frame,
                        ControlFrame::Loop { .. } | ControlFrame::Switch { .. }
                    )
                }) {
                    match frame {
                        ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. } => {
                            breaks.extend(current);
                        }
                        _ => unreachable!(),
                    }
                }
            }
            AbruptExit::Continue => {
                if let Some(ControlFrame::Loop { continues, .. }) = self
                    .control
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, ControlFrame::Loop { .. }))
                {
                    continues.extend(current);
                }
            }
            AbruptExit::Return => {}
        }
    }

    fn record_abrupt_exit(&mut self, kind: AbruptExit, environment: &FlowEnvironment) {
        for frame in self.control.iter_mut().rev() {
            if let ControlFrame::Try { abrupt_exits, .. } = frame {
                abrupt_exits.push((kind, *environment));
            }
        }
    }

    fn route_finally_abrupt(&mut self, kind: AbruptExit, environment: FlowEnvironment) {
        match kind {
            AbruptExit::Break => {
                if let Some(frame) = self.control.iter_mut().rev().find(|frame| {
                    matches!(
                        frame,
                        ControlFrame::Loop { .. } | ControlFrame::Switch { .. }
                    )
                }) {
                    match frame {
                        ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. } => {
                            breaks.push(environment);
                        }
                        _ => unreachable!(),
                    }
                }
            }
            AbruptExit::Continue => {
                if let Some(ControlFrame::Loop { continues, .. }) = self
                    .control
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, ControlFrame::Loop { .. }))
                {
                    continues.push(environment);
                }
            }
            AbruptExit::Return => {}
        }
    }
}
