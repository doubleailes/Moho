## REMOVED Requirements

### Requirement: Light linking across merged inputs
**Reason**: It served streaming by masked chunk stages (`chain::merged`,
judging lights' expressions without predicates after a merge). Moho no
longer streams; `hydra-parity-chain` ports Hydra's light linking, which
judges expressions over its input scene index whatever that index merges.
**Migration**: Run `chain::standard` over one stage. A consumer that merges
scene indices itself appends light linking after its merge.
