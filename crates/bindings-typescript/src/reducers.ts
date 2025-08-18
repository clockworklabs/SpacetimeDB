import { clientConnected, clientDisconnected, init, player, point, procedure, reducer, sendMessageSchedule, user, type Schema } from "./schema";

export const sendMessage = reducer<Schema>(
  'send_message',
  sendMessageSchedule,
  (ctx, { scheduleId, scheduledAt, text }) => {
    console.log(`Sending message: ${text} ${scheduleId}`);
  }
);

init<Schema>('init', {}, ctx => {
  console.log('Database initialized');
});

clientConnected<Schema>('on_connect', {}, ctx => {
  console.log('Client connected');
});

clientDisconnected<Schema>('on_disconnect', {}, ctx => {
  console.log('Client disconnected');
});

reducer<Schema>(
  'move_player',
  { user, foo: point, player },
  (ctx, { user, foo: point, player }): void => {
    ctx.db.player
    if (player.baz.tag === 'Foo') {
      player.baz.value += 1;
    } else if (player.baz.tag === 'Bar') {
      player.baz.value += 2;
    } else if (player.baz.tag === 'Baz') {
      player.baz.value += '!';
    }
  }
);

procedure('get_user', { user }, async (ctx, { user }) => {
  console.log(user.email);
});