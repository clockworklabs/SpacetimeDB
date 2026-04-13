import React from "react";

/**
 * A green checkmark badge for use in tables.
 *
 * Usage in MDX:
 * ```mdx
 * import { Check } from "@site/src/components/Check";
 *
 * | Feature | Supported |
 * |---------|-----------|
 * | Thing   | <Check /> |
 * ```
 */
export function Check() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="20"
      height="20"
      viewBox="0 0 20 20"
      fill="none"
      style={{ display: "block" }}
    >
      <g clipPath="url(#clip0_check)">
        <path
          d="M10 20C4.47568 20 0 15.5243 0 10C0 4.47568 4.47568 0 10 0C15.5243 0 20 4.47568 20 10C20 15.5243 15.5243 20 10 20Z"
          fill="#4CF490"
          fillOpacity="0.2"
        />
        <path
          d="M6.29781 9.30157C5.78936 8.76891 4.94537 8.74928 4.4127 9.25773C3.88004 9.76618 3.86041 10.6102 4.36886 11.1428L7.33856 14.2539C7.59015 14.5175 7.93866 14.6666 8.30303 14.6666C8.66741 14.6666 9.01591 14.5175 9.26751 14.2539L15.6311 7.58728C16.1396 7.05462 16.12 6.21063 15.5873 5.70217C15.0546 5.19372 14.2106 5.21335 13.7022 5.74601L8.30303 11.4023L6.29781 9.30157Z"
          fill="#4CF490"
        />
      </g>
      <defs>
        <clipPath id="clip0_check">
          <rect width="20" height="20" fill="white" />
        </clipPath>
      </defs>
    </svg>
  );
}

export default Check;
