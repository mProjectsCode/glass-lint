//! Control-flow state transitions for the object-flow projector.
//!
//! The fact builder emits balanced control markers. This module translates
//! those markers into snapshots and joins; it does not attempt to rediscover
//! JavaScript control flow from individual call facts.
//!
//! Joins retain only aliases and requirements proven on every reachable path.
//! Abrupt exits are held by their nearest relevant frame so `finally` and loop
//! semantics cannot accidentally make one path definite on another.

use super::{AbruptExit, ControlFrame, ControlKind, FlowEnvironment, ObjectFlowProjector};

impl ObjectFlowProjector<'_, '_> {
    /// Apply one balanced control marker to the current environment.
    pub(super) fn transfer_control(
        &mut self,
        kind: ControlKind,
        region: u32,
        _span: swc_common::Span,
    ) {
        // Control markers are consumed in the same order they were emitted;
        // reconstructing branches from nearby calls would lose empty paths.
        match kind {
            ControlKind::BranchStart
            | ControlKind::BranchThen
            | ControlKind::BranchElse
            | ControlKind::BranchEnd => self.transfer_branch(kind, region),
            ControlKind::LoopStart { .. } | ControlKind::LoopUpdate | ControlKind::LoopEnd => {
                self.transfer_loop(kind, region);
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

    fn transfer_branch(&mut self, kind: ControlKind, region: u32) {
        match kind {
            ControlKind::BranchStart => self.control.push(ControlFrame::Branch {
                region,
                base: self.environment(),
                then_exit: None,
            }),
            ControlKind::BranchThen => {
                let current = self.environment();
                if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *base = current;
                }
            }
            ControlKind::BranchElse => {
                let current = self.environment();
                let restore = if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *then_exit = Some(current);
                    Some(base.clone())
                } else {
                    None
                };
                if let Some(environment) = restore {
                    self.restore(environment);
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
                let current = self.environment();
                let joined = then_exit.as_ref().map_or_else(
                    || FlowEnvironment::join(&base, &current),
                    |then_exit| FlowEnvironment::join(then_exit, &current),
                );
                self.restore(joined);
            }
            _ => unreachable!(),
        }
    }

    fn transfer_loop(&mut self, kind: ControlKind, region: u32) {
        match kind {
            ControlKind::LoopStart { guaranteed } => self.control.push(ControlFrame::Loop {
                region,
                baseline: self.environment(),
                guaranteed,
                breaks: Vec::new(),
                continues: Vec::new(),
            }),
            ControlKind::LoopUpdate => {
                let current = self.environment();
                if let Some(ControlFrame::Loop { continues, .. }) = self.control.last()
                    && !continues.is_empty()
                {
                    let mut paths = vec![current];
                    paths.extend(continues.iter().cloned());
                    self.restore(FlowEnvironment::join_many(&paths));
                }
            }
            ControlKind::LoopEnd => {
                let Some(ControlFrame::Loop {
                    region: expected,
                    baseline,
                    guaranteed,
                    breaks,
                    continues,
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut paths = Vec::new();
                if !guaranteed {
                    paths.push(baseline);
                }
                paths.extend(breaks);
                paths.extend(continues);
                paths.push(self.environment());
                self.restore(FlowEnvironment::join_many(&paths));
            }
            _ => unreachable!(),
        }
    }

    fn transfer_switch(&mut self, kind: ControlKind, region: u32) {
        match kind {
            ControlKind::SwitchStart => self.control.push(ControlFrame::Switch {
                region,
                baseline: self.environment(),
                breaks: Vec::new(),
                has_default: false,
            }),
            ControlKind::SwitchCase { is_default } => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Switch {
                    region: expected,
                    baseline,
                    has_default,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    restore = Some(FlowEnvironment::join(&current, baseline));
                    *has_default |= is_default;
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::SwitchEnd => {
                let Some(ControlFrame::Switch {
                    region: expected,
                    baseline,
                    breaks,
                    has_default,
                    ..
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut exits = vec![self.environment()];
                exits.extend(breaks);
                if !has_default {
                    exits.push(baseline);
                }
                self.restore(FlowEnvironment::join_many(&exits));
            }
            _ => unreachable!(),
        }
    }

    fn transfer_try(&mut self, kind: ControlKind, region: u32) {
        match kind {
            ControlKind::TryStart => self.control.push(ControlFrame::Try {
                region,
                baseline: self.environment(),
                try_exit: None,
                catch_exit: None,
                normal_exit: None,
                abrupt_exits: Vec::new(),
                has_finally: false,
            }),
            ControlKind::CatchStart => {
                let current = self.environment();
                let restore = if let Some(ControlFrame::Try {
                    region: expected,
                    baseline,
                    try_exit,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *try_exit = current.is_reachable().then_some(current);
                    Some(baseline.clone())
                } else {
                    None
                };
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::FinallyStart => self.start_finally(region),
            ControlKind::TryEnd => self.end_try(region),
            _ => unreachable!(),
        }
    }

    fn start_finally(&mut self, region: u32) {
        let current = self.environment();
        let restore = if let Some(ControlFrame::Try {
            region: expected,
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            ..
        }) = self.control.last_mut()
            && *expected == region
        {
            *catch_exit = Some(current.clone());
            *has_finally = true;
            let mut normal = try_exit.clone();
            if current.is_reachable() {
                normal = Some(normal.map_or_else(
                    || current.clone(),
                    |normal| FlowEnvironment::join(&normal, &current),
                ));
            }
            normal_exit.clone_from(&normal);
            let mut incoming = normal.into_iter().collect::<Vec<_>>();
            incoming.extend(
                abrupt_exits
                    .iter()
                    .map(|(_, environment)| environment.clone()),
            );
            Some(FlowEnvironment::join_many(&incoming))
        } else {
            None
        };
        if let Some(environment) = restore {
            self.restore(environment);
        }
    }

    fn end_try(&mut self, region: u32) {
        let Some(ControlFrame::Try {
            region: expected,
            try_exit,
            catch_exit,
            normal_exit,
            abrupt_exits,
            has_finally,
            ..
        }) = self.control.pop()
        else {
            return;
        };
        if expected != region {
            return;
        }
        if has_finally {
            let after_finally = self.environment();
            for (kind, before) in abrupt_exits {
                self.apply_finally_to_abrupt_exit(kind, &before, &after_finally);
            }
            self.restore(if normal_exit.is_some_and(|normal| normal.is_reachable()) {
                after_finally
            } else {
                FlowEnvironment::unreachable()
            });
        } else if let Some(try_exit) = try_exit {
            let catch_exit = catch_exit.unwrap_or_else(|| self.environment());
            self.restore(FlowEnvironment::join(&try_exit, &catch_exit));
        }
    }

    fn transfer_abrupt(&mut self, kind: ControlKind) {
        let current = self.environment();
        let abrupt = match kind {
            ControlKind::Break => AbruptExit::Break,
            ControlKind::Continue => AbruptExit::Continue,
            ControlKind::Return => AbruptExit::Return,
            _ => unreachable!(),
        };
        self.record_abrupt_exit(abrupt, &current);
        match kind {
            ControlKind::Break => {
                if let Some(frame) = self.control.iter_mut().rev().find(|frame| {
                    matches!(
                        frame,
                        ControlFrame::Loop { .. } | ControlFrame::Switch { .. }
                    )
                }) {
                    match frame {
                        ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. } => {
                            breaks.push(current);
                        }
                        _ => unreachable!(),
                    }
                    self.reachable = false;
                }
            }
            ControlKind::Continue => {
                if let Some(ControlFrame::Loop { continues, .. }) = self
                    .control
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, ControlFrame::Loop { .. }))
                {
                    continues.push(current);
                    self.reachable = false;
                }
            }
            ControlKind::Return => self.reachable = false,
            _ => unreachable!(),
        }
    }

    fn record_abrupt_exit(&mut self, kind: AbruptExit, environment: &FlowEnvironment) {
        for frame in self.control.iter_mut().rev() {
            if let ControlFrame::Try { abrupt_exits, .. } = frame {
                abrupt_exits.push((kind, environment.clone()));
            }
        }
    }

    fn apply_finally_to_abrupt_exit(
        &mut self,
        kind: AbruptExit,
        before: &FlowEnvironment,
        after: &FlowEnvironment,
    ) {
        let frames = self.control.iter_mut().rev();
        for frame in frames {
            let targets = match (kind, frame) {
                (
                    AbruptExit::Break,
                    ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. },
                ) => Some(breaks),
                (AbruptExit::Continue, ControlFrame::Loop { continues, .. }) => Some(continues),
                _ => None,
            };
            if let Some(targets) = targets {
                for target in targets {
                    if target == before {
                        *target = after.clone();
                    }
                }
                return;
            }
        }
    }
}
