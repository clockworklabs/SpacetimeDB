import { CardLinkGrid } from "./CardLinkGrid";
// import NextJSLogo from "@site/static/images/logos/nextjs-logo.svg";
// import RemixLogo from "@site/static/images/logos/remix-logo.svg";
// import NodeJSLogo from "@site/static/images/logos/nodejs-logo.svg";
// import BunLogo from "@site/static/images/logos/bun-logo.svg";
// import Html5Logo from "@site/static/images/logos/html5-logo.svg";
import TypeScriptLogo from "@site/static/images/logos/typescript-logo.svg";
import CSharpLogo from "@site/static/images/logos/csharp-logo.svg";
import RustLogo from "@site/static/images/logos/rust-logo.svg";
import ReactLogo from "@site/static/images/logos/react-logo.svg";
// import UnityLogo from "@site/static/images/logos/unity-logo.svg";
// import UnrealLogo from "@site/static/images/logos/unreal-logo.svg";

export function QuickstartLinks() {
  return (
    <CardLinkGrid
      items={[
        {
          icon: <TypeScriptLogo height={40} />,
          href: "/quickstarts/typescript",
          docId: "intro/quickstarts/typescript",
          label: "TypeScript",
        },
        {
          icon: <CSharpLogo height={40} />,
          href: "/quickstarts/c-sharp",
          docId: "intro/quickstarts/c-sharp",
          label: "C#",
        },
        {
          icon: <RustLogo height={40} />,
          href: "/quickstarts/rust",
          docId: "intro/quickstarts/rust",
          invertIcon: true,
          label: "Rust",
        },
        {
          icon: <ReactLogo height={40} />,
          href: "/quickstarts/react",
          docId: "intro/quickstarts/react",
          label: "React",
        },
        // {
        //   icon: <UnityLogo height={40} />,
        //   href: "/quickstart/unity",
        //   docId: "quickstart/unity",
        //   label: "Unity",
        // },
        // {
        //   icon: <UnrealLogo height={40} />,
        //   href: "/quickstart/unreal",
        //   docId: "quickstart/unreal",
        //   label: "Unreal",
        // },
        // {
        //   icon: <ReactLogo height={40} />,
        //   href: "/quickstart/react",
        //   docId: "quickstart/react",
        //   label: "React",
        // },
        // {
        //   icon: <NextJSLogo height={40} />,
        //   href: "/quickstart/nextjs",
        //   docId: "quickstart/nextjs",
        //   label: "Next.js",
        // },
        // {
        //   icon: <RemixLogo height={40} />,
        //   href: "/quickstart/remix",
        //   docId: "quickstart/remix",
        //   label: "Remix",
        // },
        // {
        //   icon: <NodeJSLogo height={40} />,
        //   href: "/quickstart/nodejs",
        //   docId: "quickstart/nodejs",
        //   label: "Node.js",
        // },
        // {
        //   icon: <BunLogo height={40} />,
        //   href: "/quickstart/bun",
        //   docId: "quickstart/bun",
        //   label: "Bun",
        // },
        // {
        //   icon: <Html5Logo height={40} />,
        //   href: "/quickstart/script-tag",
        //   docId: "quickstart/script-tag",
        //   label: "Script tag",
        // },
      ]}
    />
  );
}