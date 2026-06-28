## Base class for Rust-style tagged enums (sum types) in the SpacetimeDB SDK.
##
## Rust enums can carry associated data per variant. This class stores the
## discriminant tag in [member value] and the variant's payload in [member data].
## Concrete subclasses (e.g. [ReducerOutcomeEnum]) define the valid variants
## and typed accessor methods.
##
## [b]Example:[/b]
## [codeblock]
## # ReducerOutcomeEnum extends RustEnum
## var outcome: ReducerOutcomeEnum = ReducerOutcomeEnum.create(ReducerOutcomeEnum.Options.ok, tx_msg)
## match outcome.value:
##     ReducerOutcomeEnum.Options.ok:
##         var tx: TransactionUpdateMessage = outcome.get_ok()
## [/codeblock]
class_name RustEnum
extends Resource

## The discriminant tag identifying which variant is active.
var value: int = 0
## The associated data for the active variant. Type depends on the variant.
var data: Variant
