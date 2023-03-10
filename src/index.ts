import { SpacetimeDBClient, SpacetimeDBEvent } from './spacetimedb';
import { Buffer } from "buffer";
export * from "./types";
import { LetterTile, MatchResult, Player, PlayerTile, RedeemedWord, TileAuction, TournamentPlayer, TournamentState, WinningBid } from './types';

export class EAClient {
    client: SpacetimeDBClient;

    constructor(host: string, name_or_address: string, credentials?: {identity: string, token: string}) {
        this.client = new SpacetimeDBClient(host, name_or_address, credentials);
        this.client.db.getOrCreateTable("Player", 2);
    }

    public registerAsPlayer = async (playerName: string): Promise<Player> => {
        this.client.call("register_player", [playerName]);
        return new Promise((resolve, reject) => {
            this.onRegistered(player => {
                resolve(player);
            });
        });
    }

    public makeBid = (auction_index: number, bid: number) => {
        this.client.call("make_bid", [auction_index, bid]);
    }

    public redeemWord = (tile_ids: number[]) => {
        this.client.call("redeem_word", [tile_ids]);
    }
    
    public onInitialStateSync = (cb: () => void) => {
        this.client.emitter.on("initialStateSync", cb);
    }

    public awaitInitialStateSync = async (): Promise<void> => {
        return new Promise((resolve, reject) => {
            this.onInitialStateSync(() => {
                resolve();
            })
        });
    }

    public onRegistered = (cb: (player: Player) => void) => {
        let identity = this.getCredentials()?.identity;
        if (identity) {
            for (const player of this.getAllPlayers()) {
                if (player.id === identity) {
                    cb(player);
                }
            }
        }
        this.onPlayerJoined(p => {
            if (p.id === this.getCredentials()?.identity) {
                cb(p);
            }
        })
    }

    public onTransaction = (cb: (event: SpacetimeDBEvent) => void) => {
        this.client.emitter.on("event", cb);
    }

    public onTournamentStateUpdate = (cb: (ts: TournamentState) => void) => {
        const table = this.client.db.getOrCreateTable("TournamentState");
        table.onInsert((row) => cb({
            version: row[0],
            status: row[1],
            current_match_id: row[2],
        }));
    }

    public onReceiveTile = (cb: (tile: PlayerTile) => void) => {
        const table = this.client.db.getOrCreateTable("PlayerTile");
        table.onInsert((row) => {
            cb({
                tile_id: row[0],
                player_id: row[1],
            });
        })
    }
    
    public onTournamentPlayer = (cb: (player: TournamentPlayer) => void) => {
        const table = this.client.db.getOrCreateTable("TournamentPlayer");
        table.onInsert((row) => {
            cb({
                id: row[0],
                points: row[1],
                name: row[2],
            });
        })
    }

    public onTileAuction = (cb: (auction: TileAuction) => void) => {
        const table = this.client.db.getOrCreateTable("TileAuction");
        table.onInsert((row) => {
            cb({
                auction_index: row[0],
                tile_id: row[1],
                letter: row[2],
                start_timestamp: row[3],
            });
        })
    }

    public onWinningBid = (cb: (winningBid: WinningBid) => void) => {
        const table = this.client.db.getOrCreateTable("WinningBid");
        table.onInsert((row) => {
            cb({
                auction_index: row[0],
                player_name: row[1],
                letter: row[2],
                points: row[3],
            });
        })
    }

    public onPlayerJoined = (cb: (player: Player) => void) => {
        const table = this.client.db.getOrCreateTable("Player");
        table.onInsert((row) => {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            cb({
                id: identity,
                points: row[1],
                name: row[2],
            });
        })
    }
    
    public onPlayerUpdate = (cb: (player: Player) => void) => {
        const table = this.client.db.getOrCreateTable("Player");
        table.onUpdate((row) => {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            cb({
                id: identity,
                points: row[1],
                name: row[2],
            });
        })
    }
    
    public onRedeemedWord = (cb: (word: RedeemedWord) => void) => {
        const table = this.client.db.getOrCreateTable("RedeemedWord");
        table.onInsert((row) => {
            cb({
                player_name: row[0],
                word: row[1],
                points: row[2],
                timestamp: row[3],
            });
        });
    }

    public getCredentials = (): { identity: string, token: string } | undefined => {
        if (this.client.identity) {
            return {
                identity: this.client.identity,
                token: this.client.token!
            };
        }
        return;
    }

    public getTournamentState = () => {
        const table = this.client.db.getOrCreateTable("TournamentState");
        for (const row of table.rows.values()) {
            return {
                version: row[0],
                status: row[1],
                current_match_id: row[2],
            };
        }
        return null;
    }

    public getAllPlayers = (): Player[] => {
        const players = [];
        const table = this.client.db.getOrCreateTable("Player");
        for (const row of table.rows.values()) {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            players.push({
                id: identity,
                points: row[1],
                name: row[2],
            })
        }
        return players;
    }

    public getMyPlayer = (): Player | undefined => {
        const myIdentity = this.getCredentials()?.identity;
        if (!myIdentity) {
            return;
        }
        const table = this.client.db.getOrCreateTable("Player");
        for (const row of table.rows.values()) {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            if (identity === myIdentity) {
                return {
                    id: identity,
                    points: row[1],
                    name: row[2],
                }
            }
        }
        return;
    }

    public getTileMap = (): Map<number, LetterTile> => {
        const table = this.client.db.getOrCreateTable("LetterTile");
        const tileMap = new Map();
        for (const row of table.rows.values()) {
            tileMap.set(row[0], {
                tile_id: row[0],
                letter: row[1],
                point_value: row[2]
            });
        }
        return tileMap;
    }

    public getMyTiles = (): LetterTile[] => {
        const tileMap = this.getTileMap();
        const table = this.client.db.getOrCreateTable("PlayerTile");
        const tiles: LetterTile[] = [];
        const myIdentity = this.getCredentials()?.identity;
        if (!myIdentity) {
            return tiles;
        }
        for (const row of table.rows.values()) {
            const identity = Buffer.from(row[1]['data'], 'utf8').toString('hex');
            if (identity === myIdentity) {
                tiles.push({...tileMap.get(row[0])!});
            }
        }
        return tiles;
    }

    public getWords = (): string[] => {
        const table = this.client.db.getOrCreateTable("Word");
        const words = [];
        for (const row of table.rows.values()) {
            words.push(row[0]);
        }
        return words;
    }

    public getAuctions = (): TileAuction[] => {
        const auctionTable = this.client.db.getOrCreateTable("TileAuction");
        const auctions = [];
        for (const row of auctionTable.rows.values()) {
            auctions.push({
                auction_index: row[0],
                tile_id: row[1],
                letter: row[2],
                start_timestamp: row[3],
            });
        }
        auctions.sort((a, b) => a.auction_index - b.auction_index)
            .reverse();
        return auctions;
    }

    public getWinningBids = (): WinningBid[] => {
        const winningBidTable = this.client.db.getOrCreateTable("WinningBid");
        const bids = [];
        for (const row of winningBidTable.rows.values()) {
            bids.push({
                auction_index: row[0],
                player_name: row[1],
                letter: row[2],
                points: row[3],
            });
        }
        bids.sort((a, b) => a.auction_index - b.auction_index)
            .reverse();
        return bids;
    }

    public getRedeemedWords = (): RedeemedWord[] => {
        const table = this.client.db.getOrCreateTable("RedeemedWord");
        const things = [];
        for (const row of table.rows.values()) {
            things.push({
                player_name: row[0],
                word: row[1],
                points: row[2],
                timestamp: row[3],
            });
        }
        things.sort((a, b) => b.timestamp - a.timestamp);
        return things;
    }

    public getMatchResultMap = (): Map<number, MatchResult[]> => {
        const table = this.client.db.getOrCreateTable("MatchResult");
        const map = new Map();
        for (const row of table.rows.values()) {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            const matchResult = {
                id: identity,
                points: row[1],
                name: row[2],
                match_id: row[3],
            };
            let list = map.get(matchResult.match_id);
            if (!list) {
                list = [];
                map.set(matchResult.match_id, list);
            }
            list.push(matchResult)
        }
        return map;
    }

    public getTournamentPlayers = (): TournamentPlayer[] => {
        const table = this.client.db.getOrCreateTable("TournamentPlayer");
        const things = [];
        for (const row of table.rows.values()) {
            const identity = Buffer.from(row[0]['data'], 'utf8').toString('hex');
            things.push({
                id: identity,
                points: row[1],
                name: row[2],
            });
        }
        return things;
    }
}