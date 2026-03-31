import { useEffect, useMemo, useRef } from 'react';
import cytoscape, { type Core, type ElementDefinition } from 'cytoscape';

type Props = {
  n?: number;
};

function buildCompleteGraphElements(n: number): ElementDefinition[] {
  const elements: ElementDefinition[] = [];

  for (let i = 0; i < n; i++) {
    elements.push({
      data: { id: `n${i}`, label: `${i}` },
    });
  }

  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      elements.push({
        data: {
          id: `e${i}-${j}`,
          source: `n${i}`,
          target: `n${j}`,
        },
      });
    }
  }

  return elements;
}

export default function CompleteGraphCytoscape({ n = 81 }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const cyRef = useRef<Core | null>(null);

  const elements = useMemo(() => buildCompleteGraphElements(n), [n]);

  // Create Cytoscape once
  useEffect(() => {
    if (!containerRef.current) return;

    const cy = cytoscape({
      container: containerRef.current,
      elements: [],
      style: [
        {
          selector: 'node',
          style: {
            width: 8,
            height: 8,
            label: 'data(label)',
            'font-size': 8,
            'text-valign': 'top',
            'text-margin-y': -4,
          },
        },
        {
          selector: 'edge',
          style: {
            width: 1,
            opacity: 0.12,
            'curve-style': 'straight',
          },
        },
      ],
      layout: { name: 'circle', fit: true, padding: 30 },
      minZoom: 0.01,
      maxZoom: 100,
      wheelSensitivity: 0.15,
    });

    cyRef.current = cy;

    return () => {
      cy.destroy();
      cyRef.current = null;
    };
  }, []);

  // Update graph when n changes
  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) return;

    cy.batch(() => {
      cy.elements().remove();
      cy.add(elements);
    });

    cy.layout({
      name: 'circle',
      fit: true,
      padding: 30,
      animate: false,
    }).run();
  }, [elements]);

  return (
    <div
      ref={containerRef}
      style={{ width: '100%', height: 'calc(100vh - 80px)' }}
    />
  );
}
