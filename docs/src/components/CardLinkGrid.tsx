import { CardLink, Item } from "./CardLink";

export function CardLinkGrid(props: { items: Item[] }) {
  const { items } = props;
  return (
    <div className="cards">
      {items.map((item, index) => (
        <CardLink key={index} item={item} />
      ))}
    </div>
  );
}