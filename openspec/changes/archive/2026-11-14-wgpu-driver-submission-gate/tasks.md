# Tasks

## 1. The gate

- [x] 1.1 `WgpuDriver` holds an `Arc<Mutex<()>>` gate created by `new` and shared by clones; `submit`, `present` and `configure_surface` take it
- [x] 1.2 `WgpuImage` readback submits through `WgpuDriver::submit`
- [x] 1.3 Tests: clones share the gate (a submission under a held gate waits); a readback while another clone holds the gate waits for it
- [x] 1.4 `docs/architecture.md`: the gate; fmt, clippy and workspace tests
