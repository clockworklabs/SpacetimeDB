import { describe, expect, test } from 'vitest';
import { AlgebraicType } from '../src/lib/algebraic_type';
import {
  ClientMessage,
  decodeBsatnRowList,
  RowSizeHint,
  ServerMessage,
  WebSocketMessages,
} from '../src/sdk';

describe('decodeBsatnRowList', () => {
  test('exports WebSocket protocol schemas from the SDK entry point', () => {
    expect(WebSocketMessages.ClientMessage).toBe(ClientMessage);
    expect(WebSocketMessages.ServerMessage).toBe(ServerMessage);
  });

  test('decodes fixed-width rows using the row size hint', () => {
    const rowsData = new Uint8Array([1, 0, 0, 0, 2, 0, 0, 0]);

    const rows = decodeBsatnRowList(
      {
        sizeHint: RowSizeHint.FixedSize(4),
        rowsData,
      },
      {
        elements: [{ name: 'id', algebraicType: AlgebraicType.U32 }],
      }
    );

    expect(rows).toEqual([{ id: 1 }, { id: 2 }]);
  });

  test('decodes zero-sized rows using the row offset count', () => {
    const rows = decodeBsatnRowList(
      {
        sizeHint: RowSizeHint.RowOffsets([0n, 0n]),
        rowsData: new Uint8Array(),
      },
      { elements: [] }
    );

    expect(rows).toHaveLength(2);
  });
});
