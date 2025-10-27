import { CardLink } from "./CardLink";
import CLIIcon from "@site/static/images/icons/cli-icon.svg";

export function InstallCardLink() {
    return (
        <CardLink item={{ href: "../install", label: "Install the SpacetimeDB CLI tool", icon: <CLIIcon height={40} /> }} />
    );
}