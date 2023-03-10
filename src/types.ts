export type TournamentState = {
    version: number;
    status: number;
    current_match_id: number;
};

export type MatchState = {
    id: number;
    status: number;
    skipped_first_round: boolean;
    is_last_round: boolean;
};

export type Word = {
    word: string;
};

export type Player = {
    id: string;
    points: number;
    name: string;
};

export type PlayerTile = {
    tile_id: number;
    player_id: number;
};

export type BagTile = {
    tile_id: number;
    letter: string;
};

export type LetterTile = {
    tile_id: number;
    letter: string;
    point_value: number;
};

export type TileAuction = {
    auction_index: number;
    tile_id: number;
    letter: string;
    start_timestamp: number;
};

export type PlayerBid = {
    auction_index: number;
    player_id: number;
    points: number;
    timestamp: number;
};

export type WinningBid = {
    auction_index: number,
    player_name: string,
    letter: string,
    points: number,
};

export type RedeemedWord = {
    player_name: string,
    word: string,
    points: number,
    timestamp: number,
};

export type TournamentPlayer = {
    id: string,
    points: number,
    name: string,
}

export type MatchResult = {
    id: string,
    points: number,
    name: string,
    match_id: number,
}