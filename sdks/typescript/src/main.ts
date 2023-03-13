import { SpacetimeDBClient, ProductValue, AlgebraicValue, AlgebraicType, BuiltinType, ProductTypeElement, SumType, SumTypeVariant } from "./spacetimedb";

let token: string = process.env.STDB_TOKEN || "";
let identity: string = process.env.STDB_IDENTITY || "";
let db_name: string = process.env.STDB_DATABASE || "";

// let type = AlgebraicType.createProductType([
//   new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinTypeType.String))
// ]);
// let product = AlgebraicValue.deserialize(type, ["foo"]);

class Address {
  public street: string;
  public zipcode: string;
  public country: string;

  constructor(street: string, zipcode: string, country: string) {
    this.street = street;
    this.zipcode = zipcode;
    this.country = country;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("street", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new ProductTypeElement("zipcode", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new ProductTypeElement("country", AlgebraicType.createPrimitiveType(BuiltinType.Type.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Address {
    let productValue = value.asProductValue();
		let street = productValue.elements[0].asString();
		let zipcode = productValue.elements[1].asString();
		let country = productValue.elements[2].asString();

    return new Address(street, zipcode, country);
  }
}

class Education {
  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createSumType([
      new SumTypeVariant("None", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("High", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new SumTypeVariant("Higher", AlgebraicType.createPrimitiveType(BuiltinType.Type.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Education {
    return new Education();
  }
}

class Hobby {
  public name: string;

  constructor(name: string) {
    this.name = name;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinType.Type.String))
    ]);
  }

  public static fromValue(value: AlgebraicValue): Hobby {
    let productValue = value.asProductValue();
		let name: string = productValue.elements[0].asString();

    return new Hobby(name);
  }
}

class Person {
  public name: string;
  public education: Education;
  public address: Address;
  public hobbies: Hobby[];

  constructor(name: string, education: Education, address: Address, hobbies: Hobby[]) {
    this.name = name;
    this.education = education;
    this.address = address;
    this.hobbies = hobbies;
  }

  public static getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createProductType([
      new ProductTypeElement("name", AlgebraicType.createPrimitiveType(BuiltinType.Type.String)),
      new ProductTypeElement("education", Education.getAlgebraicType()),
      new ProductTypeElement("address", Address.getAlgebraicType()),
      new ProductTypeElement("hobbies", AlgebraicType.createArrayType(Hobby.getAlgebraicType())),
    ])
  }

  public static fromValue(value: AlgebraicValue): Person {
    let productValue = value.asProductValue();
		let name: string = productValue.elements[0].asString();
		let education: Education = Education.fromValue(productValue.elements[1]);
		let address: Address = Address.fromValue(productValue.elements[2]);

    let hobbies: Hobby[] = [];
    for (let el of productValue.elements[3].asArray()) {
      hobbies.push(Hobby.fromValue(el));
    }

    return new Person(name, education, address, hobbies);
  }
}

let object = ["foo",{"1":[]},["Frankfurter Allee 53","10247","Germany"],[["programming"]]];
let v = AlgebraicValue.deserialize(Person.getAlgebraicType(), object);
console.log(v);
console.log(Person.fromValue(v));

// let client = new SpacetimeDBClient("localhost:3000", db_name, {identity: identity, token: token});
