---
name: milestone
description: Run one build milestone from docs/milestones.md end to end - spec, plan, attack tests first, implement, review, record decisions. Invoke as /milestone M1.
disable-model-invocation: true
---

Run milestone $ARGUMENTS from `docs/milestones.md`.

1. **Read.** The milestone entry, the design sections it touches in `docs/design.md`, `docs/DECISIONS.md`, and any earlier specs in `docs/specs/`.
2. **Spec.** If `docs/specs/$ARGUMENTS.md` does not exist, interview me with the AskUserQuestion tool about anything the design leaves open for this milestone: byte layouts, encodings, hash choices, limits, test cases. Do not ask what the design already answers. Then write the spec with:
   - state layouts at byte level
   - each transition's inputs, outputs and checks
   - the full list of positive tests and attack tests
   - out of scope
   - done criteria (copied from `docs/milestones.md`, made concrete)

   Stop and wait for my approval of the spec.
3. **Plan.** In plan mode, list the files to create or change and the order of work. Wait for approval.
4. **Tests first.** Write all tests from the spec. Run them and confirm the new ones fail for the right reason.
5. **Implement** until every test passes. Show the test output.
6. **Review.** Run the `covenant-reviewer` subagent on the changes. Fix must-fix and should-fix items that affect correctness or the spec. Re-run all tests.
7. **Record.** Add rows to `docs/DECISIONS.md` for every decision or deviation. Update `docs/costs.md` if sizes or mass were measured. Set the milestone status to `done` in `docs/milestones.md`.
8. **Commit** with a message naming the milestone, and summarise what was built, the test evidence and anything left open.
