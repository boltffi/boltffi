{{ class.documentation() }}final class {{ class.name() }} {
  int _handle;

  static final Finalizer<int> _finalizer = Finalizer<int>(
    (handle) => _f${{ class.release() }}(handle),
  );

  {{ class.name() }}._(this._handle) {
    _finalizer.attach(this, _handle, detach: this);
  }

  void _f$throwIfDisposed() {
    if (_handle == 0) {
      throw $$BoltException('Object has been disposed');
    }
  }

  void dispose$() {
    final handle = _handle;
    if (handle == 0) return;
    _handle = 0;
    _finalizer.detach(this);
    _f${{ class.release() }}(handle);
  }
{%- for member in class.members() %}

{{ member }}
{%- endfor %}
}
