export type GeneratorLocalState =
  | 'registered'
  | 'ready'
  | 'starting'
  | 'running';

export type CoordinatorPhase = 'idle' | 'starting' | 'measure' | 'stop';

export type GeneratorSnapshot = {
  id: string;
  hostname: string;
  desiredConnections: number;
  openedConnections: number;
  localState: GeneratorLocalState;
  activeEpoch: number | null;
};

export type EpochResult = {
  epoch: number;
  label: string | null;
  test: string;
  connector: string;
  windowSeconds: number;
  actualWindowSeconds: number;
  participantIds: string[];
  participantConnections: number;
  measuredAt: string;
  finishedAt: string;
  committedBefore: string;
  committedAfter: string;
  committedDelta: string;
  tps: number;
  verification: 'skipped' | 'passed' | 'failed';
  verificationError?: string;
  error?: string;
};

export type CoordinatorState = {
  phase: CoordinatorPhase;
  currentEpoch: number | null;
  currentLabel: string | null;
  participants: string[];
  test: string;
  connector: string;
  generators: GeneratorSnapshot[];
  lastResult: EpochResult | null;
};

export type RegisterRequest = {
  id: string;
  hostname: string;
  desiredConnections: number;
};

export type ReadyRequest = {
  id: string;
  openedConnections: number;
};

export type StartedRequest = {
  id: string;
  epoch: number;
};

export type StoppedRequest = {
  id: string;
  epoch: number;
};

export type StartEpochRequest = {
  label?: string | null;
  generatorIds?: string[];
};

export type StartEpochResponse = {
  started: boolean;
  message: string;
  state: CoordinatorState;
};
