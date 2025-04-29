import { registerReducer, registerType, type } from 'spacetimedb';

const Foo = registerType(
    'Foo',
    type.product({
        bar: type.f32,
        baz: type.string,
    })
);

console.log('hello there', new Error().stack);
try {
    function x() {
        throw new Error('hello');
    }
    x();
} catch (e) {
    // Error.captureStackTrace(e, )
    console.log('woww', e);
}
registerReducer('beepboop', [type.array(type.f32), type.bool, Foo], (x, y, z) => {
    // z.bar;
});
// console.log(registerReducer);
// registerReducer(1, [], () => {});
