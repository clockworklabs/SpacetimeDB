// Browser-safe: no server-side imports, so client bundles can share these limits.
export const FILE_BYTES_MAX = 4_000_000;
/** Default total bytes one owner may store. */
export const FILE_OWNER_BYTES_MAX = 100_000_000;
export const FILE_PATH_MAX = 1024;
export const FILE_MIME_TYPE_MAX = 127;
export const FILE_LIST_PAGE_MAX = 200;
export const FILE_VISIBILITY_OWNER = 'owner';
export const FILE_VISIBILITY_PUBLIC = 'public';

/** SenderError codes. `tooLarge` is followed by `:<bytes>/<max>`. */
export const errors = {
  invalidOwner: 'files.invalid_owner',
  invalidPath: 'files.invalid_path',
  invalidPrefix: 'files.invalid_prefix',
  invalidCursor: 'files.invalid_cursor',
  invalidPageSize: 'files.invalid_page_size',
  invalidMimeType: 'files.invalid_mime_type',
  invalidVisibility: 'files.invalid_visibility',
  tooLarge: 'files.too_large',
  quotaExceeded: 'files.quota_exceeded',
  notFound: 'files.not_found',
  pathTaken: 'files.path_taken',
} as const;
