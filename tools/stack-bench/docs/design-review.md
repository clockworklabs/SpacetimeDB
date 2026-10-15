# Stack Bench ecommerce design review

Review date: 14 October 2026.

**Stack Bench can support a controlled comparison of specified stack packages on its ecommerce workflow. Its design does not, by itself, support a general ranking of stack quality, production readiness, or development efficiency.** The main limitation is the relationship between those claims and the experiment. Fixing individual checks will improve measurement accuracy. It will not remove the limits imposed by task selection, adaptive work assignment, scoring units, and the treatment of missing results.

Guarantees are central to the benchmark's purpose. A feature that fails its required guarantees is incomplete, and blocking dependent work is intentional. The central review question is whether the selected checks establish those guarantees reliably across implementations. Because the evaluator also controls later work, an incorrect gate result can affect subsequent requests, measured coverage, and spending. Correct gate results measure delivery under this policy; they do not measure capability under equal exposure to work.

This review covers ecommerce. It distinguishes documented design choices, demonstrated runtime behavior, and implications that still need a comparison study. It makes no claim that an observed stack ranking has reversed.

## 1 The comparison bundles several causes into one result

The [study method](study-method.md) explicitly compares delivered stack packages. The packages include stack material and selected skills. The [prompting method](prompting.md#stack-guidance) states that neutral guidance can contain design advice. Its neutral guidance variants change the SpacetimeDB material while leaving the other stacks unchanged.

That is a coherent product comparison. It answers how a particular agent performs with a particular supplied package. However, a result cannot identify how much of a difference came from the database, API design, documentation, skills, development tools, or the agent's familiarity with them. These inputs change together. In the current studies the SpacetimeDB package also includes two TypeScript SDK fixes and skill edits that exist only on the benchmark branch, not in a SpacetimeDB release.

Calling the condition neutral does not remove that dependence. Equal access to the declared tools establishes procedural consistency. It does not establish that guidance has equal effects across stacks.

The design therefore has a firm attribution limit. A package-level advantage can be real while an explanation such as “the database architecture causes the advantage” remains unsupported. The existing skill variants offer a useful way to test one contributor. They do not isolate all contributors, and their mere availability is not evidence from an ablation.

**Design consequence:** use package-level claims for the standard comparison. If a decision depends on why a package wins, predefine the relevant guidance or tool contrast and measure it separately. A benchmark should not imply a causal explanation that its treatment cannot distinguish.

## 2 Guarantee gates define the measured development path

In progressive dependency mode, the agent receives ready work. Descendants remain absent from the request until prerequisites pass, including their guarantees. This is part of the baseline in the [study method](study-method.md#what-a-study-measures), and it is implemented in the [progression engine](../src/progression/dependency-mode.ts). Requiring these guarantees is consistent with the benchmark's purpose. Basic feature operation alone does not establish an acceptable foundation for later work.

Two attempts can therefore start with the same target and receive different subsequent assignments. Their observed differences include both implementation behavior and the consequences of the work-selection policy. The withheld work is missing because of an earlier outcome. It is not a random sample of the remaining task.

This remains a design property even with a perfect grader. It also remains when repairs are disabled. “No repair feedback” does not mean that the benchmark stops using outcomes to shape subsequent work. It does not follow that the agent sees a diagnostic report or can infer the withheld checks.

The current runtime makes the effect large. A controlled probe of the actual engine passed every executed check except account session reload. It used progressive selection and zero repairs:

| Selected depth | Features passed with no failure | Features passed with one persistent reload failure | Failed checks | Blocked checks | Unmeasured checks |
| --- | ---: | ---: | ---: | ---: | ---: |
| 3 | 27 of 27 | 7 of 27 | 1 | 71 | 10 |
| 6 | 49 of 49 | 7 of 49 | 1 | 138 | 17 |

These are synthetic outcomes supplied to the real state machine. They demonstrate the reach of the gate policy, not coding-agent performance or a scoring defect. The exact scripts and states are in the [local review evidence](../qualification-evidence/design-review-20261014/probe-evidence.json).

The [system design](system-design.md#terms), [check categories](check-categories.md), engine, and [tests](../tests/dependency-mode.test.ts) require guarantees to pass before dependent work opens. This is the intended policy. The size of the resulting completion loss is not evidence that the gate should be relaxed.

The remaining design concern is the influence of measurement errors. A false failure can suppress valid later work. A false pass can admit later work on a defective foundation. The privacy probe shows why confidence in a gate must depend on the boundary it actually observes. The policy also limits interpretation: blocked features were not demonstrated, but their absence does not independently establish that the agent could not implement each one.

**Design consequence:** retain guarantee gates for the intended staged-delivery question. Show the originating failure and its blocked work, and prioritize validation of checks with substantial downstream influence. Equal exposure to a complete target is a different question. The existing all-at-once mode could investigate it separately, but it is not a correction to the guarantee-based design and is not required to justify that design.

## 3 The score gives authored units the appearance of a natural scale

The [score implementation](../src/progression/dependency-score.ts) offers feature completion, check completion, weighted points, and questline averages. These are different counting rules. None establishes that its units represent equal amounts of useful software, effort, difficulty, or risk.

Feature completion counts fully passed nodes. Node boundaries are chosen by the benchmark authors. Consider a mathematical example, not an observed campaign: an app passes one of two nodes, so it receives 50%. Divide its passing node into three passing nodes while retaining the same behavior. It now passes three of four nodes, or 75%. The software has not improved. The counting scheme has changed.

The actual graph also assigns different numbers and kinds of checks to different nodes. An all-checks-required rule makes completion depend on the breadth of the assigned test set. Adding a valid check can reduce a feature score without any change to the application. This is sometimes correct, but it shows why “percentage of the product delivered” is too strong an interpretation.

Check counts have the same problem at a smaller scale. More assertions about one area can give that area more influence. Point weights make the choice explicit but do not validate it. Questline averaging introduces a further choice about how much each group matters.

The documentation already distinguishes the denominators. That protects against confusing the metrics. It does not establish that any one of them is a valid measure of product value or production risk.

**Design consequence:** describe completion as coverage of the frozen checklist. Use predefined critical outcomes and complete-target success for decisions that depend on usable delivery. Before a ranking drives a broader decision, check whether it survives reasonable alternative groupings or weights. A ranking that depends on one arbitrary partition needs that fact in its headline interpretation.

## 4 Expected production behavior combines safety rules with product preferences

The [prompting method](prompting.md#features-and-production-expectations) deliberately separates requested, expected, and observed specifications. An expected specification affects the score without appearing as an explicit specification in the request. This is a legitimate way to study defaults.

The difficulty is deciding what a default means. Unauthorized access, corrupt writes, and automatic updates in another open tab are not interchangeable expectations. The documentation itself recognizes that live state is a selected product capability rather than a universal production-readiness standard.

A combined production score can still place these properties in the same reporting category. A selected live-state guarantee can also block dependent work. That is valid when live state is part of the benchmark's chosen standard. Its inclusion must be justified as that standard, rather than treated as a universal property of all production applications.

The design mixes two questions unless the result keeps them distinct: does the app preserve necessary safety properties, and does the agent supply the benchmark's preferred capabilities without an explicit request? Success on the second question is useful information. Failure is not automatically evidence of unsafe software or inability to implement the capability.

This is also a sensitivity to requirement wording. A feature request, interface contract, or supplied skill can reveal an expectation even when its specification treatment says expected. The repository already warns about that. Treatment labels alone cannot establish how much guidance the agent actually received.

**Design consequence:** justify expected properties individually and keep safety, correctness, and selected product capabilities identifiable in the result. For a disputed capability, compare the existing requested and expected treatments with unchanged scoring. That tests the effect of explicit disclosure. It does not require disclosing attack payloads or teaching the implementation.

## 5 A deep feature graph is still one authored product workload

Ecommerce depth adds more features to the same application family. The nodes share a domain, data model, starting fixture, earlier source, and development history. Repeated builds add evidence about variation under that setup. They do not sample a population of independently selected products.

This matters even if ecommerce is the entire intended scope. Ecommerce systems differ in their interaction patterns and priorities. The selected storefront, warehouse, synchronization, and workflow requirements define one workload mix. A large number of checks does not demonstrate that this mix represents ecommerce development generally.

The [study method](study-method.md#measures-and-analysis) correctly rejects counting checks or later checkpoints as independent builds. The further design limit is task coverage. More repetitions can make the estimate for the chosen task more precise without making its scope broader.

[The Benchmark Lottery](https://arxiv.org/abs/2107.07002) shows, in other ML settings, that task selection can change relative performance. It supplies a reason to test task sensitivity here. It does not establish that Stack Bench has favored a stack or that its rankings will change.

**Design consequence:** keep claims conditional on this ecommerce workload. If a broader ecommerce claim matters, preselect a small set of materially different workloads within ecommerce. Vary meaningful requirements or interaction patterns, not just names and seed values. That is a separate study, not a prerequisite for using the current benchmark within its stated scope.

## 6 Qualification validates a test system within a chosen design

Stack Bench has substantial qualification work. The [September 29 approval](../qualification-evidence/ecommerce-l3-combined-20260929/promotion-approval.json) records full coverage validation for 112 checks, with 17 fresh and 95 retained. Claims that qualification has never happened are false.

The [qualification design](../reference-apps/README.md) binds exact reference apps, selected checks, deliberately defective controls, null controls, and runtime identities. It requires a mutation to fail at the intended observation. These rules provide valuable evidence that a known correct fixture passes and specified defects are detected.

They do not establish that the selected properties represent the intended concept, or that the test accepts all legitimate implementation families. A control set is itself an authored sample. A test and its mutation can share the same blind spot.

The stock-alert probe provides a concrete example. The privacy check rejected a visible cross-account leak but passed a leak filtered out by the page. The existing mutation removes the server's account filter. It does not combine that defect with client-side filtering. Passing qualification against that control does not establish server privacy detection across those implementations.

The design needs a clear distinction between stable execution and valid measurement. [Measurement and Fairness](https://arxiv.org/html/1912.05511v3#S3.SS2) explains why validity requires evidence that the observed measure captures its intended concept. Applied here, traceable check results do not by themselves establish a measure of production readiness.

**Design consequence:** retain qualification. Add focused challenges to the assumptions behind important checks: a different correct implementation, a different defective implementation, or an independently chosen observation of the same property. Prioritize controls that challenge an existing assumption. Repeating the same controls cannot establish what they do not test.

## 7 The measurement interface restricts which implementations are comparable

Stable controls and named operations make automated grading possible. They also add requirements beyond the user-visible product. An app must both implement the behavior and expose it through the supported measurement surface.

This creates two distinct failure risks. A missing interface can hide correct behavior from the grader. An interface that reports the expected display can conceal incorrect backend behavior. The catalog and privacy probes in the appendix demonstrate the latter problem. The credential probe demonstrates a transport coverage limit.

The [account contract](../tracks/ecommerce/contracts/accounts.md) does not require JSON authentication requests. The generic [credential patcher](../src/actions/auth-request-patch.ts) requires a supported JSON request and marks a form-encoded request inconclusive. Specialized stack paths support other forms. The resulting coverage is conditional on the implementation and adapter.

The benchmark correctly avoids inventing an application failure when the probe cannot measure it. However, the [campaign report](../src/campaigns/campaign-report.ts) calculates score distributions from completed attempts. Implementation-dependent measurement support can therefore affect the population contributing to those distributions. The assigned-attempt delivery metric retains invalid attempts, which provides an important separate view. Neither view identifies how an unmeasured implementation would have scored.

**Design consequence:** treat observability as a documented coverage boundary. Report failures of measurement separately and inspect their distribution by stack and implementation choice. Validate materially different supported implementations before claiming that the score is independent of architecture. The evidence here establishes a possible selection mechanism, not its prevalence or direction.

## 8 Cost is partly determined by how far the policy lets an app proceed

The [study method](study-method.md#measures-and-analysis) correctly keeps cost and completion separate. This avoids an especially weak composite such as dollars per passed check. A further limitation remains: a blocked attempt can be cheap because it receives less work. A successful attempt can be expensive because it reaches later work and pays for further regression checks.

Total spend is a valid measurement of running this protocol. It cannot establish the cost of completing an equal delivered scope when scopes differ. Restricting a cost comparison to successful attempts introduces another selection: each stack can have a different successful subset.

The budget policy also needs clearer reasoning. The method calls the cap a guard rather than a study condition, but says cap-hit attempts stop and can be rerun with a raised cap. It retains the stopped attempt in assigned delivery and excludes incomplete scores from completed-only distributions. Those records are useful. They do not remove the cap's effect on which outcomes are observed. Once a cap binds, its value and the replacement rule affect the experiment.

**Design consequence:** use all-assigned delivery, all-execution spend, and fixed-checkpoint costs together. Keep conditional cost summaries explicitly conditional. Predefine cap handling and report any raised-cap run as a separate allowance condition. Exact receipts establish what was spent; they cannot establish the unobserved cost of finishing a blocked or stopped attempt.

## Design decisions needed before broader claims

The current infrastructure can preserve a large amount of evidence. The unresolved issue is which interpretation that evidence supports. More machinery and more qualification runs do not settle that choice.

- State the primary question as delivery that satisfies the selected guarantees under the frozen progression policy. Keep default-behavior and equal-assignment claims within the evidence from their respective treatments.
- Retain guarantee gates. Validate their observations and effective dependencies, and distinguish the originating failure from the work it blocks. Do not reinterpret blocked work as if it had been assigned.
- Keep package-level comparisons distinct from explanations about database architecture or individual tools.
- Treat feature and check percentages as checklist measures. Assess sensitivity to their authored units before using them as broad quality rankings.
- Test a small number of decisive alternatives already supported by the system: work selection, requirement disclosure, and guidance. Freeze each comparison in advance.
- Challenge important measurement boundaries with different implementation families. Extend the affected qualification controls instead of demanding repeated full-suite runs.
- Broaden task coverage only when the intended claim requires it. Independent ecommerce variants and repeated builds address different uncertainties.

The defensible claim is specific: under a frozen ecommerce workload, agent condition, package, measurement interface, and continuation policy, these attempts delivered these measured outcomes at these recorded costs. Stronger claims need evidence for the additional step. The benchmark's design cannot make that step disappear.

## Appendix Implementation findings and evidence limits

These findings support the design critique. They remain distinct from it. Fixing them does not resolve the workload, attribution, or scoring questions above.

| Finding | Verified result | Scope |
| --- | --- | --- |
| Stock-alert privacy | The unchanged criterion passed a private record sent to another user when the page filtered its display. A visible leak failed. | Local browser probe with prepared preconditions. Database and restock setup were replaced. |
| Catalog coverage | The complete current catalog scenario passed a page showing only Air Purifier from the 13-item fixture. The wrong price failed. | Proves incomplete coverage of this feature check, not a full-benchmark bypass. |
| Progression amplification | One persistent account reload failure reduced feature completion from 27/27 to 7/27 at depth 3 and from 49/49 to 7/49 at depth 6. | Synthetic conclusive grades through the actual engine, with zero repairs. |
| Authentication transport | The existing browser test confirmed that the generic credential patch marks a form-encoded request inconclusive. | Probe support limit. It does not measure how often campaigns encounter it. |
| Draft upload metadata | A copied contract-test fixture with two prompt conditions produced an upload without their condition IDs or hashes. | Publisher as first committed (`29653199b2`). No claim about how a public website aggregates results. |
| Stock-alert privacy in campaign data | A Gemini 3.8 Flash SpacetimeDB app (job `f5f75f0e`, run 1) keeps every stock alert in a public table holding account and item ids, and filters it in the browser. It passed the stock-alert privacy criterion. The leaked rows carry no product name. | One leaderboard app inspected. Other apps were not surveyed. |
| Host load | Two Gemini studies run at the same time (about 29 sessions) had live failures that passed on an idle regrade in 3 of 28 apps. Three Argon checks timed out on a click at 15 parallel runs and passed when replayed alone. | Cause not proven. The 67 runs of other models showed no live-versus-idle difference. |
| Install-date dependence | A start script that installs packages by name rewrote `package.json` once a newer version was published. The same saved app passed the source check on one day and failed it on a later day. | One saved app. The package registry is not frozen in time. |

Relevant sources are the [stock-alert criterion](../tracks/ecommerce/scenarios/progression-stock-alerts.json), [privacy mutation](../grader/mutations/postgres-ecommerce.json), [catalog criterion](../tracks/ecommerce/scenarios/01-catalog-values.json), [catalog fixture](../tracks/ecommerce/composition/fixtures/operations.json), [authentication test](../tests/auth-request-patch.test.ts), and [publisher](../src/campaigns/campaign-publish.ts).

Reproduction files are in [design-review-20261014](../qualification-evidence/design-review-20261014/): `probe.mjs`, `probe-evidence.json`, `auth-request-patch-test.txt`, `publisher-probe.mjs`, `publisher-evidence.json`, and `publisher-probe-test.txt`. The recorded hashes for the six primary browser and progression inputs still matched when this review was written.

The isolated TypeScript build passed. The final browser and progression probes, focused authentication test, and modified publisher contract test completed. The initial privacy fixture had incorrect test setup; it was corrected, and that failed run was retained. No paid model campaigns, Docker qualification runs, or ranking comparisons were performed for these probes. Automatic approval review blocked removal of the temporary compiled review directory; it remains locally.
