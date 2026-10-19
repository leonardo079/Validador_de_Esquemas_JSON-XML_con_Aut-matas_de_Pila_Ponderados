# runtime-c

Cabeceras y soporte en C11 del validador generado. No es un crate de Cargo:
el emisor de `codegen` copia estos archivos junto al código generado.

Requisitos del código: `-std=c11 -Wall -Wextra -Werror -O2`, sin `malloc`.
