import { EAClient } from ".";
import { LetterTile } from "./types";

// Everything below this is part of an example bot that uses EAClient
const checkWord = (myTiles: LetterTile[], word: string): LetterTile[] | null => {
    const tilesChosen = [];
    const myTilesRemaining = [...myTiles];
    for (const letter of word) {
        let found = false;
        for (let i = 0; i < myTilesRemaining.length;) {
            const tile = myTilesRemaining[i];
            if (tile.letter == letter) {
                tilesChosen.push({...tile});
                myTilesRemaining.splice(i, 1);
                found = true;
                break;
            } else {
                i++;
            }
        }
        if (!found) {
            return null;
        }
    }
    return tilesChosen;
};

const findRedeemableWord = (myTiles: LetterTile[], words: string[]): LetterTile[] | null => {
    for (const word of words) {
        const result = checkWord(myTiles, word);
        if (result) {
            return result;
        }
    }
    return null;
};

const client = new EAClient("english-auction.spacetimedb.net:3000", "english-auction");
client.onInitialStateSync(async () => {
    await client.registerAsPlayer("Tyler" + Math.floor(Math.random() * 1000));
    console.log(client.getCredentials());
    console.log(client.getMyPlayer());
    client.onTileAuction(auction => {
        client.makeBid(auction.auction_index, 1);
    });

    client.onReceiveTile(tile => {
        const myTiles = client.getMyTiles();
        console.log(myTiles);
        let words = client.getWords();
        words = words.filter(word => word.length < 4);
        myTiles.sort((a, b) => a.letter.localeCompare(b.letter));
        const tiles = findRedeemableWord(myTiles, words);
        if (!tiles) {
            return;
        }
        console.log("REDEEMING: ", tiles);
        const tileIds = tiles.map(tile => tile.tile_id);
        client.redeemWord(tileIds);
    });
});

// client.onTransaction((event) => {
//     console.log(event);
// })

// client.onTournamentStateUpdate((ts) => {
//     console.log(ts);
// })


// client.onTileAuction((auction) => {
//     console.log(auction);
// })

// client.onPlayerJoined((player) => {
//     console.log(player);
// })
