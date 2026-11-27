import crypto from "node:crypto";
import { promisify } from "node:util";

const scrypt = promisify<crypto.BinaryLike, crypto.BinaryLike, number, crypto.ScryptOptions, Buffer>(crypto.scrypt);

const SCRYPT_OPTIONS = { N: 131072, r: 8, p: 1, maxmem: 256 * 1024 * 1024 };

function validPassword(password: unknown): password is string {
  return typeof password === "string" && password.length > 0 && password.length <= 64;
}

export function validCredentials(username: unknown, password: unknown): boolean {
  return typeof username === "string" && username.length > 0 && username.length <= 48
    && !/[^A-Za-z0-9-]/.test(username) && validPassword(password);
}

function derivePassword(password: string, salt: string): Promise<Buffer> {
  return scrypt(password, salt, 64, SCRYPT_OPTIONS);
}

export async function hashPassword(password: string): Promise<string> {
  if (!validPassword(password)) throw new Error("Invalid password length");
  const salt = crypto.randomBytes(16).toString("hex");
  const derived = await derivePassword(password, salt);
  return `scrypt-131072-8-1:${salt}:${derived.toString("hex")}`;
}

export async function verifyPassword(password: string, stored: string): Promise<boolean> {
  if (!validPassword(password) || typeof stored !== "string") return false;
  const match = /^scrypt-131072-8-1:([a-f0-9]{32}):([a-f0-9]{128})$/.exec(stored);
  if (!match || match[0] !== stored) return false;
  const candidate = await derivePassword(password, match[1]);
  return crypto.timingSafeEqual(candidate, Buffer.from(match[2], "hex"));
}
