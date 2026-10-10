"""Exact original timeout selection and interrupted fixture history observations."""
import re

from .executor_crash import validate_cut

TIMEOUT_CUT = 'timeout-before-fence'


def validate_timeout(value, binary_hash):
    path = value.get('timeout_path', {})
    if not isinstance(path, dict) or set(path) != {'entry','expired','deadline_call','fence_call'}:
        raise ValueError('the timeout lacks its original predicate and exact fence call')
    entry, expired = path['entry'], path['expired']
    validate_cut(entry,binary_hash,'deadline-predicate-entry')
    validate_cut(expired,binary_hash,'deadline-expired-return')
    deadline, fence = path['deadline_call'], path['fence_call']
    for snapshot in (entry,expired):
        if snapshot['original'] != value['original'] or snapshot['mapped_code'] != value['mapped_code']:
            raise ValueError('the timeout changed the original process or mapped instructions')
    for call, callee in ((deadline,entry),(fence,value)):
        caller = call.get('caller', {})
        if (set(call) != {'entry_stack_pointer','stack_return_address','caller_pc','return_virtual_address','call_bytes','caller','thread'}
            or any(type(call.get(key)) is not int or call[key] <= 0 for key in
                ('entry_stack_pointer','stack_return_address','caller_pc','return_virtual_address','thread'))
            or call['stack_return_address'] != call['caller_pc']
            or caller.get('demangled_symbol') != 'fsm_containment_authority::authority::runner::execute_cancellable'
            or not isinstance(caller.get('raw_symbol'), str) or not re.fullmatch(r'_(?:R|ZN)[A-Za-z0-9_]+',caller['raw_symbol'])
            or type(caller.get('symbol_offset')) is not int or caller['symbol_offset'] <= 0
            or type(caller.get('symbol_size')) is not int or caller['symbol_size'] <= 0
            or not caller['symbol_offset'] < call['return_virtual_address'] <= caller['symbol_offset']+caller['symbol_size']
            or not isinstance(call.get('call_bytes'), str) or not re.fullmatch('e8[a-f0-9]{8}',call['call_bytes'])):
            raise ValueError('timeout fencing came from a foreign call frame or error cleanup')
        instruction = bytes.fromhex(call['call_bytes'])
        if call['return_virtual_address'] + int.from_bytes(instruction[1:],'little',signed=True) != callee['symbol_offset']:
            raise ValueError('the original caller instruction does not target its actual predicate or fence')
    if (deadline['caller'] != fence['caller'] or deadline['thread'] != fence['thread']
        or type(expired.get('predicate_result')) is not int or expired['predicate_result'] != 1
        or type(expired.get('return_stack_pointer')) is not int
        or expired['return_stack_pointer'] != deadline['entry_stack_pointer']+8
        or type(expired.get('return_thread')) is not int or expired['return_thread'] != deadline['thread']
        or expired.get('condition') != '$al == 1 && $rsp == '+str(deadline['entry_stack_pointer']+8)
        or expired['pc'] != deadline['stack_return_address']
        or any(expired.get(key) != entry.get(key) for key in ('raw_symbol','demangled_symbol','symbol_offset'))):
        raise ValueError('the same original deadline predicate did not really return true before its runner fenced')
    return value
