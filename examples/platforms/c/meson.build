project('boltffi-c-example', 'c', default_options: ['c_std=c11'])

demo = dependency('demo', method: 'pkg-config')
example = executable('c_example', 'example.c', dependencies: demo)
test('shared library', example)

demo_static = dependency('demo-static', method: 'pkg-config')
static_example = executable('c_example_static', 'example.c', dependencies: demo_static)
test('static library', static_example)
