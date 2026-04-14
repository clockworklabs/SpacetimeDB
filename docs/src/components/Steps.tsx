import React, { ReactNode, Children, isValidElement } from "react";

interface StepTextProps {
  children: ReactNode;
}

export function StepText({ children }: StepTextProps) {
  return <div className="step-description">{children}</div>;
}

interface StepCodeProps {
  children: ReactNode;
}

export function StepCode({ children }: StepCodeProps) {
  return <div className="step-code">{children}</div>;
}

interface StepProps {
  title: string;
  children: ReactNode;
}

export function Step({ title, children }: StepProps) {
  // Separate StepText and StepCode from children
  let textContent: ReactNode = null;
  let codeContent: ReactNode = null;

  Children.forEach(children, (child) => {
    if (isValidElement(child)) {
      if (child.type === StepText) {
        textContent = child;
      } else if (child.type === StepCode) {
        codeContent = child;
      }
    }
  });

  return (
    <div className="step">
      <div className="step-content">
        <strong className="step-title">{title}</strong>
        {textContent}
      </div>
      {codeContent}
    </div>
  );
}

interface StepByStepProps {
  children: ReactNode;
}

export function StepByStep({ children }: StepByStepProps) {
  return <div className="steps">{children}</div>;
}
