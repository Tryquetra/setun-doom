# Diretrizes: Sobre os Ombros de Gigantes, Vemos Mais Longe

## 1. O lema

**Sobre os ombros de gigantes, vemos mais longe.**

O lema impõe duas obrigações. A primeira é reconhecer a origem: toda decisão começa pela pergunta de qual gigante já resolveu esse problema. A segunda é medir o avanço: "ver mais longe" é uma afirmação empírica, e afirmações empíricas exigem número.

O lema vale para pessoas e para agentes de IA (Inteligência Artificial). Ele organiza o restante deste documento.

## 2. Os gigantes

Os gigantes do projeto são conhecidos e nomeados:

- Setun e Setun-70, de Nikolai Brusentsov, pela prova de que uma máquina ternária balanceada funciona.
- Claude Shannon, pela aritmética ternária balanceada e pela teoria da informação.
- Steven Bos e os autores do REBEL-6 e do REBEL-2, pela ISA (Instruction Set Architecture, conjunto de instruções) de 32 trits e pelo pipeline de compilação.
- A tese de Bos, pela metodologia de comparação de radix e pela crítica ao argumento de economia de radix usado como prova física.
- RISC-V, pela ABI (Application Binary Interface, contrato de chamada e layout de dados), pelas convenções de syscall e pelo modelo de extensões opcionais.
- LLVM, pela infraestrutura de compilação e pelo backend de referência.
- ELF (Executable and Linkable Format, formato de objeto e executável), musl e o padrão C, pela compatibilidade de software.
- doomgeneric, pela carga de trabalho que ancora o projeto na realidade.

A regra é reutilizar a interface e inovar a execução. Antes de criar formato, ABI, biblioteca ou algoritmo, verificar se existe padrão consolidado. Quando existir, copiar o padrão e entender a instância concreta.

## 3. A realidade como medida

A primazia da realidade é o critério de verdade do projeto. O artefato observado vale mais que o argumento. A verdade é a adequação entre o que dizemos e o que existe.

Consequências operacionais:

- O frame do Doom, os 37/37 testes, os ciclos e a energia medida são a medida do progresso.
- O modelo matemático não substitui a medição física. A economia de radix é teoria; a energia no dispositivo é realidade.
- O emulador ternário e o FPGA de dual-rail são modelos binários do ternário. Os números deles precisam ser declarados como modelo, não como silício tri-estável.
- Ir ver o trabalho acontecer. Rodar o binário, olhar o frame, executar a suíte. A leitura do código e do relatório é hipótese.

Nenhuma afirmação de melhoria entra em documento, commit ou apresentação sem artefato observado. Inferência é marcada como inferência.

## 4. Padrões reais, instâncias concretas

O realismo moderado sustenta que os padrões existem, mas existem nas coisas. A especificação da ISA é real como padrão, e só vale quando as implementações a instanciam: o core, o assembler, o encoder, o simulador e os testes.

Três regras derivam disso:

1. **Fonte única.** A especificação gera encoder, tabelas do assembler, decodificador e testes. Cópias divergentes perdem o fundamento.
2. **Conformidade.** A especificação sem teste é hipótese. O teste sem especificação é acidente.
3. **Abstração ancorada.** ABI, formato de objeto e roadmap são padrões reais, verificados nos artefatos concretos.

Dois erros são proibidos. O realismo ingênuo confunde o modelo com a realidade, por exemplo concluir que o hardware ternário é mais eficiente porque o emulador ternário funciona. O nominalismo de engenharia nega os padrões e reinventa ABI, objeto e biblioteca a cada projeto.

## 5. SHUHARI: os três tempos da maestria

SHUHARI (守破離) é o modelo japonês de aquisição de maestria.

| Estágio | Sentido | No projeto |
| :-- | :-- | :-- |
| Shu | proteger a forma | ABI RISC-V, ELF, musl, LLVM, REBEL-6, 37/37 testes, frame do Doom |
| Ha | romper a forma com princípios | Backend nativo, extensões ternárias, experimento de ponteiro de 24 trits |
| Ri | transcender a forma | Computador ternário com modelo de dados e runtime próprios, nível 6 |

A disciplina é saber o estágio antes de agir. Permanecer em Shu para sempre produz a tradução RV32I como teto. Pular para Ri sem Shu produz uma ABI inventada sem domínio da provada. O Ha é a ponte e exige princípios entendidos, não apenas vontade de mudar.

## 6. Kaizen, kaikaku, kakumei: os três tempos da mudança

| Modo | Escala | No projeto |
| :-- | :-- | :-- |
| Kaizen (改善) | melhoria contínua, pequena, diária | os 27 commits do R2R, testes, determinismo, desempenho, diagnósticos |
| Kaikaku (改革) | reforma radical, por marco | backend nativo, ABI v1, formato ELF, linker com relocações |
| Kakumei (革命) | revolução, por geração | nível 6, especificação executável, verificação de tradução, view provada |

Os três se alternam. A revolução cria a base, o kaizen a explora, a reforma prepara a próxima. Kaizen sem tese vira atividade sem direção. Kaikaku sem migração quebra compatibilidade. Kakumei sem base vira recomeço permanente.

## 7. O eterno recomeço: Sísifo

Sísifo foi condenado a empurrar uma pedra até o topo, vê-la rolar de volta e recomeçar. Camus leu o mito como imagem da condição humana: o mundo não responde ao desejo de sentido, e a revolta consiste em continuar com lucidez.

No projeto, o mito nomeia uma condição e um risco. A condição é a recorrência: não existe versão final, toda entrega cria manutenção e todo ciclo deixa trabalho para o próximo. O risco é transformar o recomeço em método, reescrevendo do zero o que funcionava e repetindo o mesmo erro com ferramenta nova.

O mito sugere seis disciplinas:

1. Deixar marca no fim de cada ciclo: teste, especificação, ADR, ferramenta, medida.
2. Reduzir o peso da pedra: complexidade, acoplamento, formato proprietário, dívida de documentação.
3. Reduzir a inclinação da colina: automação, geração, integração contínua, cache, determinismo.
4. Distinguir iteração de repetição. Iteração parte do ciclo anterior. Repetição parte do zero.
5. Não romantizar o retrabalho. "Imaginar Sísifo feliz" é postura filosófica individual, não política de empresa.
6. Aceitar a recorrência sem aceitar a estagnação. A altura medida a cada ciclo substitui a ideia de topo.

Para pessoas: não recomeçar o que pode ser continuado, documentar para que o próximo comece mais alto e medir a altura a cada ciclo.

Para agentes de IA: preservar contexto, não reescrever do zero quando existe padrão consolidado, acumular mudanças verificáveis, registrar decisões e medir o progresso entre ciclos.

## 8. A reversibilidade: chapéu, corte de cabelo, tatuagem

O processo de decisão é proporcional ao custo de desfazer. A classificação mede reversibilidade, não importância.

| Classe | Custo de desfazer | Quem decide | Registro |
| :-- | :-- | :-- | :-- |
| Chapéu | imediato, baixo | qualquer pessoa | nenhum, ou uma linha |
| Corte de cabelo | tempo e esforço, período de crescimento | pequeno grupo | razão curta e data de revisão |
| Tatuagem | alto ou impossível | consenso explícito | ADR (Architecture Decision Record, registro de decisão de arquitetura), versão e migração |

Exemplos no projeto:

- **Chapéu:** janela de registradores, internos do backend, implementação do linker, formato do `--stats`, nomes de flags, documentação.
- **Corte de cabelo:** escolha do backend, formato de objeto textual ou ELF, modelo de memória, threads, ABI de ponto flutuante.
- **Tatuagem:** largura de ponteiro, tamanho de `char`, endianness, convenção de chamada, congelamento do `abi-v0`, formato de objeto depois de distribuído, contrato de artefato do nível 6.

A armadilha do corte de cabelo é o período de crescimento maior que o horizonte do projeto. Um formato de objeto que leva dois anos para migrar funciona como tatuagem.

## 9. Protocolo de decisão

Toda decisão relevante passa por seis perguntas, nesta ordem:

| Passo | Pergunta | Saída |
| --: | :-- | :-- |
| 1 | Qual é o estágio SHUHARI? | Shu, Ha ou Ri |
| 2 | Qual é o tempo da mudança? | kaizen, kaikaku ou kakumei |
| 3 | Qual é a classe de reversibilidade? | chapéu, corte ou tatuagem |
| 4 | Existe evidência observada? | artefato, teste, medição ou inferência marcada |
| 5 | Reaproveita um gigante? | padrão existente ou justificativa de invenção |
| 6 | Qual é o registro? | nota, razão curta ou ADR |

Decisões de chapéu não sobem para revisão. Decisões de tatuagem não descem para decisão individual. O protocolo existe para impedir os dois erros.

## 10. Diretrizes para pessoas

- Assumir o resultado, não a atividade.
- Decidir na classe correta. Não escalar chapéus. Não decidir tatuagens sozinho.
- Escrever ADR para tatuagem, razão curta para corte de cabelo.
- Revisar o artefato, não apenas o diff. Rodar o binário e olhar o frame.
- Manter a camada de compatibilidade viva enquanto a camada nativa é aposta.
- Medir o agregado, não a contagem de commits.
- Nomear o gigante usado. Dar crédito é parte do método.
- Quando a realidade contradizer o plano, atualizar o plano.

## 11. Diretrizes para agentes de IA

- Ler a especificação, o contexto do repositório e as decisões registradas antes de agir.
- Classificar a decisão antes de executar. Tatuagem exige aprovação explícita.
- Fazer mudanças pequenas, verificáveis e reversíveis. Um commit por melhoria.
- Rodar o artefato, não apenas os testes. Teste não é prova de comportamento.
- Nunca afirmar resultado não observado. Marcar inferência.
- Nunca suprimir falha para fazer teste passar.
- Reaproveitar padrão consolidado. Justificar invenção.
- Atualizar documentação, mapa e registro na mesma mudança.
- Reportar falha com comando e saída.
- Respeitar as regras de linguagem e de estilo do projeto.
- Escalar decisões de tatuagem quando não houver aprovação humana disponível.

## 12. O que evitamos

- Tratar tatuagem como chapéu, mudando ABI quando já existem binários.
- Tratar chapéu como tatuagem, paralisando em decisões reversíveis.
- Confundir o modelo com a realidade, tomando o emulador pelo hardware.
- Negar os padrões, reinventando o que já existe.
- Permanecer em Shu como desculpa para nunca construir o caminho nativo.
- Saltar para Ri sem dominar a forma.
- Anunciar vantagem sem número.
- Usar o lema como slogan sem compromisso de medição.

## 13. Encerramento

O lema funciona como compromisso operacional. Sobre os ombros de gigantes significa reutilizar o que foi provado. Vemos mais longe significa medir o que o projeto acrescenta.

SHUHARI diz em que estágio estamos. Kaizen, kaikaku e kakumei dizem em que ritmo mudamos. Chapéu, corte de cabelo e tatuagem dizem quanto custa voltar atrás. A primazia da realidade e o realismo moderado dizem como sabemos que estamos certos.

O projeto avança quando as cinco dimensões concordam: o estágio, o ritmo, a reversibilidade, a recorrência e a evidência.
