import { CardLink } from "./CardLink";
import CLIIcon from "@site/static/images/icons/cli-icon.svg";

export function InstallCardLink() {
    return (
        <div style={{ maxWidth: 400 }}>
            <CardLink item={{ href: "https://spacetimedb.com/install", label: "Install the SpacetimeDB CLI tool", icon: <CLIIcon height={40} /> }} />
        </div>
    );
}