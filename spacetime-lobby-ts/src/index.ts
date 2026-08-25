// Registered SpacetimeDB exports for direct module publication.

export { default, init } from './submodule/schema';
export {
  add_admin_identity,
  cancel_ticket,
  close_room,
  expire_tickets,
  get_lobby_status,
  join_ranked_queue,
  join_queue,
  join_room,
  leave_room,
  lobbyAdminMatchResults,
  lobbyAdminRoomSeats,
  lobbyAdminRooms,
  lobbyAdminTickets,
  lobbyQueueSummary,
  lobbyRankedLeaderboard,
  myLobbyRatings,
  myLobbyRoomSeats,
  myLobbyRooms,
  myLobbyTickets,
  remove_admin_identity,
  set_rating,
  update_config,
} from './submodule/operations';
