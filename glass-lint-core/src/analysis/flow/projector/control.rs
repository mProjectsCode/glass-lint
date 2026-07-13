//! Control-flow state transitions for the object-flow projector.
//!
//! The fact builder emits balanced control markers. This module translates
//! those markers into snapshots and joins; it does not attempt to rediscover
//! JavaScript control flow from individual call facts.

use super::{AbruptExit, ControlFrame, ControlKind, FlowEnvironment, ObjectFlowProjector};

impl ObjectFlowProjector<'_, '_> {
    pub(super) fn transfer_control(
        &mut self,
        kind: ControlKind,
        region: u32,
        _span: swc_common::Span,
    ) {
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
                let mut restore = None;
                if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *then_exit = Some(current);
                    restore = Some(base.clone());
                }
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
                // The current environment is the else path when one exists,
                // and the then path otherwise (for an if without an else).
                let joined = then_exit.as_ref().map_or_else(
                    || FlowEnvironment::join(&base, &current),
                    |then_exit| FlowEnvironment::join(then_exit, &current),
                );
                self.restore(joined);
            }
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
                // A continue in a do/while still reaches its test and can
                // therefore reach the loop exit.
                paths.extend(continues);
                paths.push(self.environment());
                self.restore(FlowEnvironment::join_many(&paths));
            }
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
                    has_default: default,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    // The current environment is the fall-through input
                    // from the preceding case. Joining it with baseline
                    // also admits direct entry at this case.
                    restore = Some(FlowEnvironment::join(&current, baseline));
                    *default |= is_default;
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
                let mut restore = None;
                if let Some(ControlFrame::Try {
                    region: expected,
                    baseline,
                    try_exit,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *try_exit = current.reachable.then_some(current);
                    restore = Some(baseline.clone());
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::FinallyStart => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Try {
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
                    if current.reachable {
                        normal = Some(normal.map_or(current.clone(), |normal| {
                            FlowEnvironment::join(&normal, &current)
                        }));
                    }
                    *normal_exit = normal.clone();
                    let mut incoming = Vec::new();
                    if let Some(normal) = normal {
                        incoming.push(normal);
                    }
                    incoming.extend(
                        abrupt_exits
                            .iter()
                            .map(|(_, environment)| environment.clone()),
                    );
                    restore = Some(FlowEnvironment::join_many(&incoming));
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::TryEnd => {
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
                    if let Some(normal) = normal_exit {
                        if normal.reachable {
                            self.restore(after_finally);
                        } else {
                            self.restore(FlowEnvironment::unreachable());
                        }
                    } else {
                        self.restore(FlowEnvironment::unreachable());
                    }
                    return;
                }
                if let Some(try_exit) = try_exit {
                    let catch_exit = catch_exit.unwrap_or_else(|| self.environment());
                    self.restore(FlowEnvironment::join(&try_exit, &catch_exit));
                }
            }
            ControlKind::Break => {
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Break, &current);
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
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Continue, &current);
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
            ControlKind::Return => {
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Return, &current);
                self.reachable = false;
            }
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
