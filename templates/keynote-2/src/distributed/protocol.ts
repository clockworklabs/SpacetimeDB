export type GeneratorLocalState = 'registered' | 'ready' | 'running';

export type CoordinatorPhase = 'idle' | 'warmup' | 'measure' | 'stop';

export type DistributedLoadOptions = {
  pipelined: boolean;
  maxInflightPerConnection: number;
};

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
  loadOptions: DistributedLoadOptions;
  warmupSeconds: number;
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
  loadOptions: DistributedLoadOptions;
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
