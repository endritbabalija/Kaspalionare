---
paths:
  - "covenants/**"
---

# Covenant rules

- Every transition lists its exact inputs, outputs and output indices, and checks each one: role, round, script template and KIP-20 covenant binding. Reject any extra output. One output must never satisfy two obligations.
- Successor state is built in script from bytes in the transaction (R1). Encodings are canonical, length-delimited and domain-separated before hashing.
- Arithmetic is checked signed 64-bit integer math. No floating point. Division rounds down; remainders go where the design says. Write down the maximum value of every quantity.
- Time checks are lower bounds only (lock time). Never rely on a transaction expiring.
- Every covenant change comes with the attack tests listed in its milestone spec, written before the implementation.
- When script size or mass changes, record the new numbers in `docs/costs.md`.
