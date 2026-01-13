import { and, eq, isNotNull, isNull, lte } from 'drizzle-orm';
import type { PostgresJsDatabase } from 'drizzle-orm/postgres-js';
import { messages, scheduledMessages } from './db/schema';

function stripScheduledPrefix(content: string): string {
  const m = content.match(/^\s*scheduled:\s*(.+)$/i);
  return m ? m[1].trim() : content;
}

export function startJobs(
  db: PostgresJsDatabase<Record<string, never>>,
  realtime: {
    broadcastMessageCreated(roomId: number, messageId: number): void;
    broadcastMessageDeleted(roomId: number, messageId: number): void;
  },
) {
  const scheduledTimer = setInterval(async () => {
    const due = await db
      .select()
      .from(scheduledMessages)
      .where(
        and(
          isNull(scheduledMessages.cancelledAt),
          isNull(scheduledMessages.sentAt),
          lte(scheduledMessages.sendAt, new Date()),
        ),
      )
      .limit(20);

    for (const job of due) {
      try {
        const content = stripScheduledPrefix(job.content);
        const [inserted] = await db
          .insert(messages)
          .values({
            roomId: job.roomId,
            authorId: job.authorId,
            content,
          })
          .returning();

        await db
          .update(scheduledMessages)
          .set({ sentAt: new Date() })
          .where(eq(scheduledMessages.id, job.id));

        if (inserted) realtime.broadcastMessageCreated(job.roomId, inserted.id);
      } catch {
        // If something goes wrong, try again next tick.
      }
    }
  }, 1000);

  const ephemeralTimer = setInterval(async () => {
    const expired = await db
      .select({ id: messages.id, roomId: messages.roomId })
      .from(messages)
      .where(and(isNotNull(messages.expiresAt), lte(messages.expiresAt, new Date())))
      .limit(50);

    for (const msg of expired) {
      try {
        await db.delete(messages).where(eq(messages.id, msg.id));
        realtime.broadcastMessageDeleted(msg.roomId, msg.id);
      } catch {
        // ignore and try again next tick
      }
    }
  }, 2000);

  function stop() {
    clearInterval(scheduledTimer);
    clearInterval(ephemeralTimer);
  }

  return { stop };
}

