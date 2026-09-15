import type { Express, RequestHandler } from 'express';
import mongoose, { Schema, Types } from 'mongoose';
import { Profile } from './progression-models.js';
import { User } from './models.js';

const Book = mongoose.model('AddressBook', new Schema({
  _id: { type: Schema.Types.ObjectId, required: true },
  entries: [{ name: { type: String, default: '' }, address: { type: String, default: '' }, isDefault: Boolean }],
}));

async function ensureBook(userId: Types.ObjectId) {
  const profile = await Profile.findOne({ userId }).lean();
  const entries = profile && (profile.name !== '' || profile.address !== '')
    ? [{ _id: new Types.ObjectId(), name: profile.name, address: profile.address, isDefault: true }] : [];
  await Book.updateOne({ _id: userId }, { $setOnInsert: { entries } }, { upsert: true });
}

export async function initializeAddressBook() {
  await Book.init();
  for (const user of await User.find().select('_id')) await ensureBook(user._id);
}

function text(name: unknown, address: unknown) {
  if (typeof name !== 'string' || typeof address !== 'string') throw new Error('Name and address must be text');
  return { name, address };
}

async function withBook<T>(userId: Types.ObjectId, write: boolean, work: (book: any) => T | Promise<T>): Promise<T> {
  await ensureBook(userId);
  return mongoose.connection.transaction(async session => {
    const book = await Book.findById(userId).session(session);
    if (!book) throw new Error('Address book missing');
    const result = await work(book);
    if (write) {
      await book.save({ session });
      const selected = book.entries.find(entry => entry.isDefault);
      await Profile.updateOne({ userId }, { $set: { name: selected?.name ?? '', address: selected?.address ?? '' } },
        { session, upsert: true });
    }
    return result;
  });
}

export async function saveDefaultAddress(userId: Types.ObjectId, name: string, address: string) {
  const value = text(name, address);
  await withBook(userId, true, book => {
    const entry = book.entries.find((entry: any) => entry.isDefault);
    if (entry) Object.assign(entry, value);
    else book.entries.push({ ...value, isDefault: true });
  });
}

export function registerAddressBook(app: Express, auth: RequestHandler, changed: (userId?: string) => void) {
  const route = (method: 'get' | 'post' | 'put' | 'delete', path: string,
    work: (book: any, id: string, body: any) => unknown) => {
    app[method](path, auth, async (req: any, res, next) => {
      try {
        const result = await withBook(req.progressionUser._id, method !== 'get',
          book => work(book, String(req.params.id ?? ''), req.body));
        if (method !== 'get') changed(String(req.progressionUser._id));
        res.json(result);
      } catch (error) {
        if (error instanceof Error && ['Address missing', 'Choose another default first', 'Name and address must be text'].includes(error.message)) {
          res.status(error.message === 'Address missing' ? 404 : 400).json({ error: error.message });
        } else next(error);
      }
    });
  };
  const json = (entry: any) => ({ id: String(entry._id), name: entry.name, address: entry.address, isDefault: entry.isDefault });
  route('get', '/api/addresses', book => ({ entries: book.entries.map(json) }));
  route('get', '/api/addresses/:id', (book, id) => {
    const entry = book.entries.find((entry: any) => String(entry._id) === id);
    if (!entry) throw new Error('Address missing');
    return json(entry);
  });
  route('post', '/api/addresses', (book, _id, body) => {
    const entry = { _id: new Types.ObjectId(), ...text(body?.name, body?.address), isDefault: book.entries.length === 0 };
    book.entries.push(entry);
    return { id: String(entry._id) };
  });
  for (const operation of ['edit', 'default', 'delete'] as const) {
    route(operation === 'delete' ? 'delete' : 'put', `/api/addresses/:id${operation === 'default' ? '/default' : ''}`,
      (book, id, body) => {
        const entry = book.entries.find((entry: any) => String(entry._id) === id);
        if (!entry) throw new Error('Address missing');
        if (operation === 'edit') Object.assign(entry, text(body?.name, body?.address));
        else if (operation === 'default') for (const candidate of book.entries) candidate.isDefault = String(candidate._id) === id;
        else {
          if (entry.isDefault && book.entries.length > 1) throw new Error('Choose another default first');
          book.entries = book.entries.filter((entry: any) => String(entry._id) !== id);
        }
        return { ok: true };
      });
  }
}
