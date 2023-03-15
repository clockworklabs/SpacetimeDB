import { SpacetimeDBClient, ProductValue, AlgebraicValue, AlgebraicType, BuiltinType, ProductTypeElement, SumType, SumTypeVariant, DatabaseTable, ClientDB } from "./spacetimedb";

let token: string = process.env.STDB_TOKEN || "";
let identity: string = process.env.STDB_IDENTITY || "";
let db_name: string = process.env.STDB_DATABASE || "";

// let type = AlgebraicType.createProductType([
//   new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinTypeType.String))
// ]);
// let product = AlgebraicValue.deserialize(type, ["foo"]);

class InventoryItem {
  public item_id: number;

  constructor(item_id: number) {
    this.item_id = item_id;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("item_id", AlgebraicType.createPrimitiveType(BuiltinType.Type.I64))
    ]);
  }

  public static fromValue(value: AlgebraicValue): InventoryItem {
    let productValue = value.asProductValue();
		let item_id = productValue.elements[0].asNumber();

    return new InventoryItem(item_id);
  }
}

class Inventory {
  public gold: number;
  public items: InventoryItem[];

  constructor(gold: number, items: InventoryItem[]) {
    this.gold = gold;
    this.items = items;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("gold", AlgebraicType.createPrimitiveType(BuiltinType.Type.U64)),
      new ProductTypeElement("items", AlgebraicType.createArrayType(InventoryItem.getAlgebraicType()))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Inventory {
    let productValue = value.asProductValue();
		let gold = productValue.elements[0].asNumber();
    let items: InventoryItem[] = [];
    for (let el of productValue.elements[1].asArray()) {
      items.push(InventoryItem.fromValue(el));
    }

    return new Inventory(gold, items);
  }
}

class Class {
  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createSumType([
      new SumTypeVariant("Barbarian", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("Sorceress", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("Druid", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("Rogue", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("Necromancer", AlgebraicType.createPrimitiveType(BuiltinType.Type.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Class {
    return new Class();
  }
}

class Player extends DatabaseTable {
  public static tableName = "Player";
  private static clientDB: ClientDB = global.clientDB;
  public name: string;
  public _class: Class;
  public inventory: Inventory;
  public avatar: Uint8Array;

  constructor(name: string, _class: Class, inventory: Inventory, avatar: Uint8Array) {
    super();

    this.name = name;
    this._class = _class;
    this.inventory = inventory;
    this.avatar = avatar;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new ProductTypeElement("_class", Class.getAlgebraicType()),
      new ProductTypeElement("inventory", Inventory.getAlgebraicType()),
      new ProductTypeElement("avatar", AlgebraicType.createArrayType(AlgebraicType.createPrimitiveType(BuiltinType.Type.U8))),
    ])
  }

  public static fromValue(value: AlgebraicValue): Player {
    let productValue = value.asProductValue();
		let name: string = productValue.elements[0].asString();
		let _class: Class = Class.fromValue(productValue.elements[1]);
		let inventory: Inventory = Inventory.fromValue(productValue.elements[2]);
    let avatar: Uint8Array = productValue.elements[3].asBytes();

    return new Player(name, _class, inventory, avatar);
  }

  public static count(): number {
    return this.clientDB.getTable("Player").count();
  }
}

// let object = ["foo",{"1":[]},["Frankfurter Allee 53","10247","Germany"],[["programming"]]];
// let v = AlgebraicValue.deserialize(Person.getAlgebraicType(), object);
// console.log(v);
// console.log(Person.fromValue(v));

// the next line would be generated for each table to automatically subscribe and process entities in cache
global.entityClasses.set("Player", Player);
clientDB.getOrCreateTable("Player", undefined, Player);
let client = new SpacetimeDBClient("localhost:3000", db_name, {identity: identity, token: token});

setTimeout(() => {
  let table = client.db.tables.get('Player');
  if (table !== undefined) {
    console.log(table.rows);
  }

  console.log(Player.count());
}, 500);
