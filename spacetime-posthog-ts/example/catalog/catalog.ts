export interface VariantSeed {
  variantId: string;
  productId: string;
  name: string;
  flavor: string;
  contextTokens: number;
  reasoning: number;
  latency: number;
  priceCents: number;
  discountBps?: number;
  active?: boolean;
  featured?: boolean;
}

export interface ProductSeed {
  productId: string;
  name: string;
  category: string;
  description: string;
  baseAppeal: number;
  active?: boolean;
  variants: VariantSeed[];
}

export interface ScenarioSeed {
  scenarioId: string;
  name: string;
  description: string;
  trafficPerTick: number;
  priceSensitivity: number;
  rushBias: number;
  researchBias: number;
  visualBias: number;
  memoryBias: number;
  premiumBias: number;
  volatility: number;
}

export const PRODUCTS: ProductSeed[] = [
  {
    productId: 'context_cooler',
    name: 'Context Cooler',
    category: 'context',
    description:
      'A tall glass of extra working memory for agents with long prompts.',
    baseAppeal: 68,
    variants: [
      {
        variantId: 'context_cooler_classic',
        productId: 'context_cooler',
        name: 'Classic Context',
        flavor: 'vanilla',
        contextTokens: 64000,
        reasoning: 4,
        latency: 5,
        priceCents: 900,
      },
      {
        variantId: 'context_cooler_raspberry',
        productId: 'context_cooler',
        name: 'Raspberry Long Context',
        flavor: 'raspberry',
        contextTokens: 180000,
        reasoning: 6,
        latency: 4,
        priceCents: 1900,
        discountBps: 500,
        featured: true,
      },
    ],
  },
  {
    productId: 'reasoning_refresher',
    name: 'Reasoning Refresher',
    category: 'quality',
    description:
      'Extra thinking syrup for bots that refuse to be wrong in public.',
    baseAppeal: 72,
    variants: [
      {
        variantId: 'reasoning_refresher_smart',
        productId: 'reasoning_refresher',
        name: 'Smart Syrup',
        flavor: 'blueberry',
        contextTokens: 96000,
        reasoning: 8,
        latency: 3,
        priceCents: 2400,
      },
      {
        variantId: 'reasoning_refresher_deep',
        productId: 'reasoning_refresher',
        name: 'Deep Thought Double',
        flavor: 'espresso',
        contextTokens: 220000,
        reasoning: 10,
        latency: 2,
        priceCents: 3900,
      },
    ],
  },
  {
    productId: 'speed_spritz',
    name: 'Speed Spritz',
    category: 'latency',
    description: 'Cold, fizzy priority inference for bots in a hurry.',
    baseAppeal: 66,
    variants: [
      {
        variantId: 'speed_spritz_priority',
        productId: 'speed_spritz',
        name: 'Priority Lime',
        flavor: 'lime',
        contextTokens: 48000,
        reasoning: 4,
        latency: 9,
        priceCents: 1400,
      },
    ],
  },
  {
    productId: 'vision_fizz',
    name: 'Vision Fizz',
    category: 'multimodal',
    description: 'Sparkling image support for bots staring at screenshots.',
    baseAppeal: 62,
    variants: [
      {
        variantId: 'vision_fizz_snapshot',
        productId: 'vision_fizz',
        name: 'Snapshot Soda',
        flavor: 'grape',
        contextTokens: 80000,
        reasoning: 5,
        latency: 5,
        priceCents: 1700,
      },
    ],
  },
  {
    productId: 'memory_mint',
    name: 'Memory Mint',
    category: 'memory',
    description:
      'Persistent memory with a clean finish and fewer repeated questions.',
    baseAppeal: 58,
    variants: [
      {
        variantId: 'memory_mint_sticky',
        productId: 'memory_mint',
        name: 'Sticky Mint',
        flavor: 'mint',
        contextTokens: 120000,
        reasoning: 5,
        latency: 4,
        priceCents: 2100,
      },
    ],
  },
  {
    productId: 'tool_tonic',
    name: 'Tool Tonic',
    category: 'tools',
    description: 'Function-calling bubbles for agents with things to do.',
    baseAppeal: 64,
    variants: [
      {
        variantId: 'tool_tonic_fizz',
        productId: 'tool_tonic',
        name: 'Tool Fizz',
        flavor: 'ginger',
        contextTokens: 90000,
        reasoning: 6,
        latency: 6,
        priceCents: 1600,
      },
    ],
  },
];

export const SCENARIOS: ScenarioSeed[] = [
  {
    scenarioId: 'steady_shift',
    name: 'Steady Shift',
    description:
      'A normal cafe shift with mixed robot traffic and balanced preferences.',
    trafficPerTick: 4,
    priceSensitivity: 45,
    rushBias: 35,
    researchBias: 35,
    visualBias: 22,
    memoryBias: 24,
    premiumBias: 24,
    volatility: 18,
  },
  {
    scenarioId: 'launch_rush',
    name: 'Launch Rush',
    description:
      'A product launch sends impatient agents sprinting for priority inference.',
    trafficPerTick: 8,
    priceSensitivity: 28,
    rushBias: 70,
    researchBias: 34,
    visualBias: 24,
    memoryBias: 20,
    premiumBias: 42,
    volatility: 32,
  },
  {
    scenarioId: 'budget_bots',
    name: 'Budget Bots',
    description:
      'A coupon crowd wants lots of compute and hates sticker shock.',
    trafficPerTick: 6,
    priceSensitivity: 82,
    rushBias: 26,
    researchBias: 28,
    visualBias: 18,
    memoryBias: 22,
    premiumBias: 10,
    volatility: 24,
  },
  {
    scenarioId: 'research_lab',
    name: 'Research Lab',
    description:
      'Deep-work agents prefer long context, high reasoning, memory, and quality.',
    trafficPerTick: 5,
    priceSensitivity: 25,
    rushBias: 18,
    researchBias: 76,
    visualBias: 28,
    memoryBias: 62,
    premiumBias: 58,
    volatility: 16,
  },
];
