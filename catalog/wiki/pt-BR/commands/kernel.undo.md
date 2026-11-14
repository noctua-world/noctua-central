# Desfazer

Desfazer volta atrás no seu último passo. Um passo é uma coisa que você pediu
ao Noctua, com tudo o que ela mudou de uma vez: os valores que gravou e os
arquivos que criou.

## O que ele desfaz

- cada valor que o passo mudou, devolvido ao que era antes;
- os arquivos que o passo criou, removidos quando nenhum outro passo os usa.

## Quando ele para

Desfazer para, sem mudar nada, quando:

- o passo foi além deste aparelho, como uma mensagem enviada ou um pedido a
  outro serviço, e nada foi registrado para desfazer isso;
- algo mudou um dos valores do passo depois dele, e devolvê-lo desfaria também
  essa mudança posterior;
- você não pode mais fazer uma das coisas que o passo fez. Desfazer pergunta de
  novo, como você é agora, antes de devolver qualquer coisa.

## Refazer

Refazer traz de volta o passo que você desfez, desde que você não tenha feito
nada novo depois.
