# Moho

A renderer-agnostic scene index over [openusd](https://github.com/mxpv/openusd)
stages, in the model of Hydra 2.0. Moho sits between a composed USD stage and
the renderers that draw it: it hands out prims as lazily evaluated data
sources, reports what changed down to the data locator, and owns the time
everything is read at.

It is named after the Mohorovičić discontinuity, the boundary between the
Earth's crust and mantle. [Mantle](https://github.com/doubleailes/mantle), the
DCC, feeds it. [Crust](https://github.com/doubleailes/crust-render), the path
tracer, and Obsidian, the GPU viewport, consume it.

Status: bootstrap. See `ROADMAP.md` and `docs/architecture.md`.

## License

MIT
