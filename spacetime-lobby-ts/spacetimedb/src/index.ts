import spacetimedb from '../../src/submodule/schema';
import { install } from '../../src/submodule/install';
export {
  addLobbyAdmin,
  cancelTicket,
  getLobbyStatus,
  joinQueue,
  joinRoom,
  leaveRoom,
  lobbyAdminMatchResults,
  lobbyAdminRoomSeats,
  lobbyAdminRooms,
  lobbyAdminTickets,
  lobbyQueueSummary,
  lobbyRankedLeaderboard,
  lobbySweep,
  myLobbyRatings,
  myLobbyRoomSeats,
  myLobbyRooms,
  myLobbyTickets,
  removeLobbyAdmin,
  setRating,
  updateConfig,
} from '../../src/submodule/operations';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  install(ctx);
});
