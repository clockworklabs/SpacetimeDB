import { ConvexCredentials } from '@convex-dev/auth/providers/ConvexCredentials';
import { convexAuth, createAccount, retrieveAccount } from '@convex-dev/auth/server';
import { ConvexError } from 'convex/values';
import { Scrypt } from 'lucia';
import { makeFunctionReference } from 'convex/server';

export const { auth, signIn, signOut, store } = convexAuth({
  providers: [ConvexCredentials({
    id: 'password',
    crypto: {
      hashSecret: (password) => new Scrypt().hash(password),
      verifySecret: (password, hash) => new Scrypt().verify(hash, password),
    },
    async authorize(params, ctx) {
      if (typeof params.username !== 'string' || !/^[A-Za-z0-9-]{1,48}$/.test(params.username)) {
        throw new ConvexError('Invalid username');
      }
      if (typeof params.password !== 'string' || params.password.length > 64) {
        throw new ConvexError('Invalid password');
      }
      const account = { id: params.username, secret: params.password };
      if (params.flow === 'signUp') {
        // createAccount atomically returns the new OR existing account. Only this
        // request's server-generated marker proves it created the user.
        const registrationNonce = crypto.randomUUID();
        const { user } = await createAccount(ctx, {
          provider: 'password', account,
          profile: { name: params.username, registrationNonce },
          shouldLinkViaEmail: false, shouldLinkViaPhone: false,
        });
        if (user.registrationNonce !== registrationNonce) throw new ConvexError('Username already exists');
        await ctx.runMutation(makeFunctionReference('accounts:ensure'), { userId: user._id });
        return { userId: user._id };
      }
      if (params.flow !== 'signIn') throw new ConvexError('Invalid authentication flow');
      const result = await retrieveAccount(ctx, { provider: 'password', account });
      if (!result) throw new ConvexError('Invalid credentials');
      await ctx.runMutation(makeFunctionReference('accounts:ensure'), { userId: result.user._id });
      return { userId: result.user._id };
    },
  })],
});
