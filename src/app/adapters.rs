//! Plan 13-08 populates this file. `Adapters` will hold trait objects for all
//! infra ports (CommandRunnerPort, MetroPort, PortProbePort, WorktreePort,
//! DevicePort, JiraPort, MultiplexerPort) so `update()` and `effect_runner`
//! can dispatch via a single injected bundle.
#![allow(dead_code)]

pub struct Adapters;
