{{ class.documentation() }}final class {{ class.name() }} {
  int _handle;
  int _f$activeCalls = 0;
  bool _f$disposed = false;

  static final Finalizer<int> _finalizer = Finalizer<int>(
    (handle) => _f${{ class.release() }}(handle),
  );

  {{ class.name() }}._(this._handle) {
    _finalizer.attach(this, _handle, detach: this);
  }

  void _f$throwIfDisposed() {
    if (_f$disposed) {
      throw $$BoltException('Object has been disposed');
    }
  }

  void dispose$() {
    if (_f$disposed) return;
    _f$disposed = true;
    if (_f$activeCalls == 0) _f$release();
  }

  void _f$beginCall() {
    _f$throwIfDisposed();
    _f$activeCalls++;
  }

  void _f$endCall() {
    _f$activeCalls--;
    if (_f$activeCalls == 0 && _f$disposed) _f$release();
  }

  void _f$release() {
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
