# Verificar histórico de atividades

O Noctua registra cada ação que faz por você em um histórico de atividades.
Cada registro é ligado ao anterior, então um registro alterado, removido ou
fora de ordem quebra a ligação seguinte.

Verificar o histórico lê esse histórico desde o início, ou desde um registro
que você indicar, e confere cada ligação. Não muda nada.

## O que ele responde

- quantos registros conferiu, quando todas as ligações estão certas;
- senão, o primeiro registro cuja ligação não confere, e por quê.

## Quando uma ligação não confere

Uma ligação quebrada quer dizer que o histórico foi alterado depois de escrito.
O Noctua não o conserta, porque nada pode dizer qual versão é a verdadeira.
Fale com quem instalou o seu Noctua.
