## Context

UsdImaging chains point instancer propagation before native instancer
propagation, and translates instance-proxy paths afterwards only for
material bindings; a point instancer prototype inside an instance has no
counterpart there to follow. Moho chains native instancing first, so its
point instancing filter can find the proxied prim in the native
instancer's propagated prototype.

## Decisions

### D1. Resolving through the propagated prototype

For a prototype target the input does not hold, the nearest ancestor
carrying `instance` names its native instancer; that instancer's topology
names the propagated prototype; the target maps to the same relative path
inside it. That prim's data, re-rooted from there to the copy, is the
copy's.

### D2. Copies under the instancer

A copy under the propagated prototype would be drawn by every native
instance of that prototype. Proxy copies go under the instancer instead,
at `I/ForInstancer<hash>/prototype<i>`, `i` the prototype's index; the
`ForInstancer<hash>` scope is a typeless prim holding them (named
`ForInstancer<hash>_<n>` should the instancer already have a child of that
name). Targets the index holds keep their copies at `P/ForInstancer<hash>`.
The scope is notified added and removed with the copies it holds when the
model is rebuilt.
