#pragma once

namespace SpacetimeDB {
// Omission preserves the host default: Public ordinarily, Private when scheduled.
enum class FunctionVisibility { Public, Private, Internal };
}
