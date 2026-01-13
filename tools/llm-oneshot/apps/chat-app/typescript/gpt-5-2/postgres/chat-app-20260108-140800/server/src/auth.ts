import type { Request, Response, NextFunction } from 'express';
import jwt from 'jsonwebtoken';

export type AuthTokenPayload = {
  userId: string;
};

const JWT_SECRET = process.env.JWT_SECRET || '';

export function requireJwtSecret(): string {
  if (!JWT_SECRET) {
    throw new Error('Missing JWT_SECRET env var');
  }
  return JWT_SECRET;
}

export function signToken(payload: AuthTokenPayload): string {
  return jwt.sign(payload, requireJwtSecret(), { expiresIn: '30d' });
}

export function verifyToken(token: string): AuthTokenPayload {
  const decoded = jwt.verify(token, requireJwtSecret());
  if (!decoded || typeof decoded !== 'object' || typeof (decoded as any).userId !== 'string') {
    throw new Error('Invalid token payload');
  }
  return decoded as AuthTokenPayload;
}

export type AuthedRequest = Request & { userId: string };

export function authMiddleware(req: Request, res: Response, next: NextFunction) {
  const header = req.header('authorization') || '';
  const m = header.match(/^Bearer\s+(.+)$/i);
  if (!m) return res.status(401).json({ error: 'Unauthorized' });
  try {
    const payload = verifyToken(m[1]);
    (req as AuthedRequest).userId = payload.userId;
    return next();
  } catch {
    return res.status(401).json({ error: 'Unauthorized' });
  }
}

export function tokenFromSocketAuth(auth: unknown): string | null {
  if (!auth || typeof auth !== 'object') return null;
  const token = (auth as any).token;
  return typeof token === 'string' && token.length > 0 ? token : null;
}

