import React from "react";
import Admonition from "@theme/Admonition";

export function CppModuleVersionNotice(): JSX.Element {
  return (
    <Admonition type="important" title="C++ Modules and SpacetimeDB 2.0">
      <p>
        SpacetimeDB <code>2.0</code> is available for other module languages, but C++ server modules
        are currently pinned to <code>v1.12.0</code>. If you are following the C++ tab in this guide,
        use the <code>v1.12.0</code> release track for now.
      </p>
    </Admonition>
  );
}
