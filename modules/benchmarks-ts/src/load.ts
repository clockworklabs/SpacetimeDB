export type Load = {
  initialLoad: number,
  smallTable: number,
  numPlayers: number,
  bigTable: number,
  biggestTable: number,
};

export function newLoad(initialLoad: number): Load {
  return {
    initialLoad,
    smallTable: initialLoad,
    numPlayers: initialLoad,
    bigTable: initialLoad * 50,
    biggestTable: initialLoad * 100,
  };
}

export function blackBox(x: any) {
  // TODO: actually do something to defeat optimizations?
}
