/** SenderError codes thrown by the auth reducers and procedures. */
export const errors = {
  notAuthenticated: 'auth.not_authenticated',
  notAuthorized: 'auth.not_authorized',
  configMissing: 'auth.config_missing',
  signingKeyRequired: 'auth.signing_key_required',
  invalidPrivateKeyPem: 'auth.invalid_private_key_pem',
  invalidToken: 'auth.invalid_token',
  sessionNotFound: 'auth.session_not_found',
  sessionNotOwned: 'auth.session_not_owned',
  userNotFound: 'auth.user_not_found',
  nameTooLong: 'auth.name_too_long',
  imageTooLong: 'auth.image_too_long',
  lastAdmin: 'auth.last_admin',
} as const;
