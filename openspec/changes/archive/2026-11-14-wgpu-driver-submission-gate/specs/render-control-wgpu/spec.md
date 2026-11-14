## ADDED Requirements

### Requirement: One submission gate per device
The wgpu driver SHALL carry one gate shared by every clone of it, and SHALL
submit command buffers, present surface images and reconfigure surfaces
under it, so none of them overlaps another on the driver's device. Its
readbacks SHALL submit under it too.

#### Scenario: A reconfiguration waits for a submission
- **WHEN** one thread holds a clone of the host's driver and submits through it while another reconfigures a surface through another clone
- **THEN** the reconfiguration starts only once the submission returned, and the other way round

#### Scenario: A renderer and its host
- **WHEN** a host hands its driver to a renderer and both submit through their clones
- **THEN** they take the same gate
