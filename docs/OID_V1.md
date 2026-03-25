
1. OID_ROOT is 1 (0x1)
2. OID_PKG_ROOT is 1.1
3. OID_PKG_ROOT_POS (PersonalOS) is 1.1.1 equivalent to @personalos/
4. OID_PKG_ROOT_POS_PUBLIC is 1.1.1.1 equivlaent to @personalos/[pkgpath]
5. OID_PKG_ROOT_POS_INTERNAL is 1.1.1.2 equivalent to @personalos/internal
6. OID_PKG_ROOT_POS_SYSTEM is 1.1.1.3 equivalent to @personalos/system

these special roots also have special security modes, system can invoke VM-escape and full CR. internal can only use VM-escape.

7. 1.1.2 is the root of the public package tree, the encoding changes to 1.1.2.[octal count x 2][3-bit aligned hash of a-z-. reverse dns name] [custom package OID]
This last format may change if it would overflow the 63x2 address scheme.

---

❯ right but there's more than that, let's see what I can remember:

yes, the basic useState returns two values, a value and a setter. The value is more like a read-only bind though.
The React hook system is basically built on top of useRef and we use that the same way as our primitive.
More complex hooks may return n values this is the hook-return value.
The compiler treats all bound values as ordinal values in the component/package/subpackage boundary. This allows for a simple mapping of bound values which can be 'exploded' or spread when referenced by a
caller. This works by having a bitmap representing which of the values are used from a specific return value (unused values like _ are 0s in this bitmap), then these are treated in order through the entire
subpackage boundary. When the bound value is passed as a parameter, it's referencing the same bound value, which also is allocated into the appropriate slot in the UI ops.

This allows for a very efficient compiler to VM since the rules act like SSA.

The same applies within an object-value (the TKV in our layout), each property or subproperty is sorted in lexicographical order as an ordinal.

For OIDs representing object types (in the memory system) we also use a further ordinal value which acts as the index into the array or object-value, or rowid. This gives use a 3x63 address which maps into
VDBE (same rule, MSB is 0 in each 64-bit component)
