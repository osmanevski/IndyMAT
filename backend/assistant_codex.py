"""Panel lifecycle for one persistent app-server per Codex conversation."""
import threading
import uuid
from backend.codex_app_server import CodexAppServer
from backend.i18n import tr


class CodexConversations:
    def _codex_new_client(self, identity, session, executable):
        generation = uuid.uuid4().hex
        with self.lock:
            if self.closed or self.sessions.get(identity) is not session or not session['running']: return None, None
            session['codex_generation'] = generation
            session['codex_finished'] = []
        from backend.assistants import child_environment
        def created(child):
            with self.lock:
                abandoned = self.closed or self.sessions.get(identity) is not session or not session['running'] or session.get('codex_generation') != generation
                if not abandoned: session['codex_client'] = child
            if abandoned: child.close()
        client = CodexAppServer(executable, session['folder'], child_environment(secret=self.secret),
            lambda event: self._codex_event(identity, session, generation, event),
            lambda kind, payload: self._codex_request(identity, session, generation, kind, payload), on_created=created)
        with self.lock:
            abandoned = self.closed or self.sessions.get(identity) is not session or not session['running'] or session.get('codex_generation') != generation
        if abandoned:
            client.close()
            return None, None
        return client, generation

    def _codex_launch(self, identity, session, executable, prompt, steering=False):
        # Caller allocated the application turn under the manager lock before any RPC.
        turn = session['turn']
        def run():
            with session['codex_rpc_lock']:
                client = None
                try:
                    with self.lock:
                        if self.closed or self.sessions.get(identity) is not session: return
                        if session['turn'] != turn or not session['running']:
                            if steering: self._emit(session, {'type': 'notice', 'text': tr('Codex turn ended before this message could be sent; send it again.')})
                            return
                        client = session.get('codex_client')
                        generation = session.get('codex_generation')
                    if client is None:
                        client, generation = self._codex_new_client(identity, session, executable)
                        if client is None: return
                        thread = session['conversation']
                        if thread:
                            with self.lock: session['codex_resuming'] = True
                            try: thread = client.resume_thread(thread)
                            except Exception:
                                self._emit(session, {'type': 'notice', 'text': tr('Codex could not resume the previous thread; starting a new thread.')})
                                self._codex_discard(identity, session, generation)
                                with self.lock: session['codex_resuming'] = False
                                client, generation = self._codex_new_client(identity, session, executable)
                                if client is None: return
                                thread = None
                            finally:
                                with self.lock: session['codex_resuming'] = False
                        if not thread:
                            bridge = session.get('bridge')
                            servers = {'indymat': {'command': bridge['python'], 'args': [bridge['script'], bridge['capability']], 'default_tools_approval_mode': 'approve'}} if bridge else None
                            thread = client.start_thread('workspace-write' if session['mode'] == 'edit' else 'read-only',
                                'on-request' if session['mode'] == 'ask' else 'never', session['developer_instructions'], servers)
                        with self.lock:
                            if session.get('codex_generation') != generation: return
                            session['conversation'] = thread
                    with self.lock:
                        if self.closed or self.sessions.get(identity) is not session: return
                        if session['turn'] != turn or not session['running']:
                            if steering: self._emit(session, {'type': 'notice', 'text': tr('Codex turn ended before this message could be sent; send it again.')})
                            return
                        thread = session['conversation']
                        remote_turn = session.get('codex_turn')
                        stopped = session['stopped']
                    if steering:
                        if not stopped and remote_turn: client.steer(thread, remote_turn, prompt)
                        return
                    if stopped:
                        self._codex_finish(identity, session, 'stopped')
                        return
                    remote_turn = client.start_turn(thread, prompt)
                    with self.lock:
                        current = session['turn'] == turn and session.get('codex_generation') == generation and session['running']
                        if current: session['codex_turn'] = remote_turn
                        stopped = current and session['stopped']
                    if stopped: client.interrupt(thread, remote_turn)
                except Exception as error:
                    with self.lock:
                        current = self.sessions.get(identity) is session and session['turn'] == turn and session['running']
                    if current:
                        self._emit(session, {'type': 'notice' if steering else 'error', 'text': str(error)})
                        if not steering:
                            self._codex_discard(identity, session, session.get('codex_generation'))
                            self._codex_finish(identity, session, 'failed')

        threading.Thread(target=run, daemon=True).start()

    def _codex_finish(self, identity, session, state):
        with self.lock:
            if not session['running']: return
            remote = session.get('codex_turn')
            if remote: session['codex_finished'] = (session.get('codex_finished', []) + [remote])[-128:]
            state = 'stopped' if session['stopped'] else state
            self._emit(session, {'type': 'turn-end', 'state': state, 'conversation': session['conversation']})
            session['running'] = False
            self.approvals.cancel(identity, tr('The assistant turn is no longer running.'))

    def _codex_discard(self, identity, session, generation):
        with self.lock:
            if session.get('codex_generation') != generation: return
            client = session.pop('codex_client', None)
            session['codex_generation'] = None
            self.approvals.cancel(identity, tr('The assistant turn is no longer running.'))
        if client: client.close()

    def _codex_event(self, identity, session, generation, event):
        with self.lock:
            if self.sessions.get(identity) is not session or session.get('codex_generation') != generation or self.closed: return
            if not session['running'] and not event.get('terminal'): return
            kind = event.get('type')
            if kind == 'conversation':
                session['conversation'] = event['conversation']
                return
            if event.get('turn_id'):
                if event['turn_id'] in session.get('codex_finished', []): return
                if session.get('codex_turn') and event['turn_id'] != session['codex_turn']: return
                session['codex_turn'] = event['turn_id']
            visible = {key: event[key] for key in ('type', 'text', 'name') if key in event}
            if event.get('terminal'):
                client = session.pop('codex_client', None)
                session['codex_generation'] = None
                if session.get('codex_resuming'):
                    self._emit(session, {**visible, 'type': 'notice'})
                else:
                    self._emit(session, visible)
                    self._codex_finish(identity, session, 'failed')
            elif kind == 'turn-end':
                self._codex_finish(identity, session, event['state'])
            else:
                self._emit(session, {**visible, 'type': 'notice'} if event.get('will_retry') else visible)
        if event.get('terminal') and client: client.close()

    def _codex_request(self, identity, session, generation, kind, payload):
        with self.lock:
            if self.sessions.get(identity) is not session or session.get('codex_generation') != generation: return 'cancel'
            if payload.get('turn_id') in session.get('codex_finished', []): return 'cancel'
            if session['mode'] != 'ask': return 'decline'
            if payload.get('turn_id'): session['codex_turn'] = payload['turn_id']
            turn = session['turn']
        return self.approvals.request_prepared(identity, turn, kind, payload)

    def _codex_stop(self, session):
        # No manager lock while waiting for the child. A starting turn observes stopped itself.
        with self.lock:
            client = session.get('codex_client')
            thread, turn = session['conversation'], session.get('codex_turn')
        if not thread or not turn:
            identity = session['identity']
            self._codex_discard(identity, session, session.get('codex_generation'))
            self._codex_finish(identity, session, 'stopped')
        elif client:
            try: client.interrupt(thread, turn)
            except Exception:
                identity = session['identity']
                self._codex_discard(identity, session, session.get('codex_generation'))
                self._codex_finish(identity, session, 'stopped')
